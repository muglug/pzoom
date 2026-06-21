//! Class-like declaration scanning (class / interface / trait / enum).
//!
//! Mirrors Hakana's `code_info_builder/classlike_scanner.rs`. These methods belong
//! to [`DeclarationCollector`]; split out of the module root for organization.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::{AnonymousClass, Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::modifier::Modifier;
use pzoom_str::StrId;

use pzoom_code_info::class_like_info::{ClassLikeInfo, ClassLikeKind, TemplateType};
use pzoom_code_info::class_type_alias::ClassTypeAlias;
use pzoom_code_info::{GenericParent, TAtomic, TUnion};
use rustc_hash::FxHashMap;

use super::DeclarationCollector;

impl<'a, 'p> DeclarationCollector<'a, 'p> {
    pub(crate) fn visit_class(&mut self, class: &Class<'_>) {
        let name = self.make_fqn(class.name.value);
        let span = class.span();
        let mut class_docblock_type_aliases: FxHashMap<String, TUnion> = FxHashMap::default();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Class,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            name_location: {
                let name_span = class.name.span();
                Some((name_span.start.offset, name_span.end.offset))
            },
            conditional_guard_classes: self.current_guard_classes.clone(),
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_final = self.is_docblock_final(parsed);
            info.is_public_api =
                parsed.tags.contains_key("psalm-api") || parsed.tags.contains_key("api");
            info.is_immutable = self.is_docblock_immutable(parsed);
            info.specialize_instance = parsed.tags.contains_key("psalm-taint-specialize");
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(parsed);
            info.is_consistent_constructor = self.is_docblock_consistent_constructor(parsed);
            info.enforce_template_inheritance = self.is_docblock_consistent_templates(parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(parsed);
            info.is_deprecated = self.is_docblock_deprecated(parsed);
            info.deprecation_message = self.get_docblock_deprecation_message(parsed);
            info.internal =
                self.get_docblock_internal_scopes(parsed, info.name, &mut info.docblock_issues);
            info.is_internal = !info.internal.is_empty();
            self.validate_type_alias_docblock_tags(&mut info, parsed, span.start.offset);
        }

        if self.has_attribute_named(&class.attribute_lists, "Deprecated") {
            info.is_deprecated = true;
        }

        // The `#[AllowDynamicProperties]` attribute permits assigning/reading undeclared
        // properties, so the property set is unsealed (matching Psalm).
        if self.has_attribute_named(&class.attribute_lists, "AllowDynamicProperties") {
            info.no_seal_properties = true;
        }

        info.attribute_flags = self.get_attribute_flags(name, &class.attribute_lists);
        info.attributes = self.collect_attributes(&class.attribute_lists);

        // Parse modifiers
        for modifier in &class.modifiers {
            match modifier {
                Modifier::Final(_) => info.is_final = true,
                Modifier::Abstract(_) => info.is_abstract = true,
                Modifier::Readonly(_) => info.is_readonly = true,
                _ => {}
            }
        }

        if info.is_readonly {
            info.is_immutable = true;
            info.is_external_mutation_free = true;
        }

        // Parse extends (class can only extend one class)
        if let Some(extends) = &class.extends
            && let Some(parent) = extends.types.first()
        {
            let parent_name = self.resolve_identifier(parent);
            info.parent_class = Some(parent_name);
        }

        // Parse implements
        if let Some(implements) = &class.implements {
            for iface in &implements.types {
                let iface_name = self.resolve_identifier(iface);
                info.interfaces.insert(iface_name);
            }
        }

        let parent_class = info.parent_class;
        self.precollect_class_constants(&mut info, &class.members, Some(name), parent_class);

        if let Some(parsed) = parsed_docblock.as_ref() {
            let template_bindings = self.parse_docblock_template_bindings(
                parsed,
                GenericParent::ClassLike(name),
                Some(name),
                info.parent_class,
                None,
                Some(&info.constants),
                &mut info.docblock_issues,
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                    variance: binding.variance,
                })
                .collect();
        }

        if let Some(parsed) = parsed_docblock.as_ref() {
            let class_template_map = self.build_template_map_from_class_template_types(
                &info.template_types,
                GenericParent::ClassLike(name),
            );
            // Psalm reads `@psalm-type` aliases from every comment attached
            // to the node (ClassLikeNodeScanner's getComments loop), so
            // collect across the whole preceding docblock run — the closest
            // block is the run's last entry, its aliases winning on collision.
            for run_parsed in self.find_preceding_docblock_run(span.start.offset) {
                class_docblock_type_aliases = self.collect_docblock_type_aliases(
                    &run_parsed,
                    Some(name),
                    info.parent_class,
                    Some(&class_template_map),
                    Some(&class_docblock_type_aliases),
                );
            }

            for (alias_name, aliased_type) in &class_docblock_type_aliases {
                let scoped_alias = self.interner.intern(&format!(
                    "{}::{}",
                    self.interner.lookup(name),
                    alias_name
                ));
                self.declarations.type_aliases.push(ClassTypeAlias {
                    name: scoped_alias,
                    aliased_type: aliased_type.clone(),
                    file_path: self.file_path,
                    start_offset: span.start.offset,
                });
            }
            self.register_namespace_type_aliases(&class_docblock_type_aliases, span.start.offset);

            let previous_aliases = std::mem::replace(
                &mut self.active_docblock_type_aliases,
                class_docblock_type_aliases.clone(),
            );
            let parent_class = info.parent_class;
            self.apply_docblock_mixins(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );
            self.apply_docblock_template_extends(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );
            self.apply_docblock_requirements(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_properties(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_methods(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );

            // `@psalm-yield T`: the promised type produced by `yield`ing an
            // instance of this class (Psalm's ClassLikeStorage::$yield).
            if let Some(yield_tags) = parsed.tags.get("psalm-yield")
                && let Some(content) = yield_tags.values().next()
            {
                let type_str = content.trim();
                if !type_str.is_empty()
                    && let Ok(parsed_type) =
                        crate::docblock::parse_type_string(type_str, self.interner.parent_ref())
                {
                    info.yield_type = Some(self.resolve_docblock_union_type(
                        parsed_type,
                        Some(name),
                        parent_class,
                        Some(&class_template_map),
                    ));
                }
            }

            self.apply_docblock_inheritors(
                &mut info,
                parsed,
                Some(name),
                parent_class,
                Some(&class_template_map),
            );

            self.active_docblock_type_aliases = previous_aliases;
        }

        // Parse members
        let previous_aliases = std::mem::replace(
            &mut self.active_docblock_type_aliases,
            class_docblock_type_aliases,
        );
        self.collect_class_members(&mut info, &class.members);
        self.active_docblock_type_aliases = previous_aliases;
        self.add_old_style_constructor_alias(&mut info);

        self.declarations.classes.push(info);
    }

    /// Register an anonymous class as a real classlike storage under its
    /// synthetic `@anonymous-class:{file}:{offset}` name (Psalm registers
    /// `{parent}@anonymous` storages in ReflectorVisitor). Class-level
    /// docblock processing is skipped: a docblock preceding `new class`
    /// belongs to the enclosing statement, not the class.
    pub(crate) fn visit_anonymous_class(&mut self, class: &AnonymousClass<'_>) {
        let span = class.span();
        let name = self.interner.intern(&format!(
            "{}:{}:{}",
            pzoom_code_info::ANONYMOUS_CLASS_PREFIX,
            self.interner.lookup(self.file_path),
            span.start.offset
        ));

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Class,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            conditional_guard_classes: self.current_guard_classes.clone(),
            // Anonymous classes cannot be extended.
            is_final: true,
            ..Default::default()
        };

        for modifier in class.modifiers.iter() {
            if let Modifier::Readonly(_) = modifier {
                info.is_readonly = true;
                info.is_immutable = true;
                info.is_external_mutation_free = true;
            }
        }

        if let Some(extends) = &class.extends
            && let Some(parent) = extends.types.first()
        {
            info.parent_class = Some(self.resolve_identifier(parent));
        }

        if let Some(implements) = &class.implements {
            for iface in &implements.types {
                let iface_name = self.resolve_identifier(iface);
                info.interfaces.insert(iface_name);
            }
        }

        let parent_class = info.parent_class;
        self.precollect_class_constants(&mut info, &class.members, Some(name), parent_class);
        self.collect_class_members(&mut info, &class.members);
        self.add_old_style_constructor_alias(&mut info);

        self.declarations.classes.push(info);
    }

    pub(crate) fn visit_interface(&mut self, iface: &Interface<'_>) {
        let name = self.make_fqn(iface.name.value);
        let span = iface.span();
        let mut class_docblock_type_aliases: FxHashMap<String, TUnion> = FxHashMap::default();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Interface,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            name_location: {
                let name_span = iface.name.span();
                Some((name_span.start.offset, name_span.end.offset))
            },
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(parsed);
            info.specialize_instance = parsed.tags.contains_key("psalm-taint-specialize");
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(parsed);
            info.is_deprecated = self.is_docblock_deprecated(parsed);
            info.deprecation_message = self.get_docblock_deprecation_message(parsed);
            info.internal =
                self.get_docblock_internal_scopes(parsed, info.name, &mut info.docblock_issues);
            info.is_internal = !info.internal.is_empty();
            self.validate_type_alias_docblock_tags(&mut info, parsed, span.start.offset);
        }

        if self.has_attribute_named(&iface.attribute_lists, "Deprecated") {
            info.is_deprecated = true;
        }

        // Parse extends (interfaces can extend multiple interfaces)
        if let Some(extends) = &iface.extends {
            for parent in &extends.types {
                let parent_name = self.resolve_identifier(parent);
                info.interfaces.insert(parent_name);
            }
        }

        self.precollect_class_constants(&mut info, &iface.members, Some(name), None);

        if let Some(parsed) = parsed_docblock.as_ref() {
            let template_bindings = self.parse_docblock_template_bindings(
                parsed,
                GenericParent::ClassLike(name),
                None,
                None,
                None,
                Some(&info.constants),
                &mut info.docblock_issues,
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                    variance: binding.variance,
                })
                .collect();
        }

        if let Some(parsed) = parsed_docblock.as_ref() {
            let class_template_map = self.build_template_map_from_class_template_types(
                &info.template_types,
                GenericParent::ClassLike(name),
            );
            // Psalm reads `@psalm-type` aliases from every comment attached
            // to the node (ClassLikeNodeScanner's getComments loop), so
            // collect across the whole preceding docblock run — the closest
            // block is the run's last entry, its aliases winning on collision.
            for run_parsed in self.find_preceding_docblock_run(span.start.offset) {
                class_docblock_type_aliases = self.collect_docblock_type_aliases(
                    &run_parsed,
                    Some(name),
                    None,
                    Some(&class_template_map),
                    Some(&class_docblock_type_aliases),
                );
            }

            for (alias_name, aliased_type) in &class_docblock_type_aliases {
                let scoped_alias = self.interner.intern(&format!(
                    "{}::{}",
                    self.interner.lookup(name),
                    alias_name
                ));
                self.declarations.type_aliases.push(ClassTypeAlias {
                    name: scoped_alias,
                    aliased_type: aliased_type.clone(),
                    file_path: self.file_path,
                    start_offset: span.start.offset,
                });
            }
            self.register_namespace_type_aliases(&class_docblock_type_aliases, span.start.offset);

            let previous_aliases = std::mem::replace(
                &mut self.active_docblock_type_aliases,
                class_docblock_type_aliases.clone(),
            );
            self.apply_docblock_mixins(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_template_extends(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_requirements(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_properties(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_methods(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );

            // `@psalm-yield T`: the promised type produced by `yield`ing an
            // instance of this interface (Psalm's ClassLikeStorage::$yield).
            if let Some(yield_tags) = parsed.tags.get("psalm-yield")
                && let Some(content) = yield_tags.values().next()
            {
                let type_str = content.trim();
                if !type_str.is_empty()
                    && let Ok(parsed_type) =
                        crate::docblock::parse_type_string(type_str, self.interner.parent_ref())
                {
                    info.yield_type = Some(self.resolve_docblock_union_type(
                        parsed_type,
                        Some(name),
                        None,
                        Some(&class_template_map),
                    ));
                }
            }

            self.apply_docblock_inheritors(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );

            self.active_docblock_type_aliases = previous_aliases;
        }

        // Parse members
        let previous_aliases = std::mem::replace(
            &mut self.active_docblock_type_aliases,
            class_docblock_type_aliases,
        );
        self.collect_class_members(&mut info, &iface.members);
        self.active_docblock_type_aliases = previous_aliases;

        self.declarations.classes.push(info);
    }

    pub(crate) fn visit_trait(&mut self, tr: &Trait<'_>) {
        let name = self.make_fqn(tr.name.value);
        let span = tr.span();
        let mut class_docblock_type_aliases: FxHashMap<String, TUnion> = FxHashMap::default();

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Trait,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            name_location: {
                let name_span = tr.name.span();
                Some((name_span.start.offset, name_span.end.offset))
            },
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(parsed);
            info.specialize_instance = parsed.tags.contains_key("psalm-taint-specialize");
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(parsed);
            info.is_deprecated = self.is_docblock_deprecated(parsed);
            info.deprecation_message = self.get_docblock_deprecation_message(parsed);
            info.internal =
                self.get_docblock_internal_scopes(parsed, info.name, &mut info.docblock_issues);
            info.is_internal = !info.internal.is_empty();
            self.validate_type_alias_docblock_tags(&mut info, parsed, span.start.offset);
        }

        if self.has_attribute_named(&tr.attribute_lists, "Deprecated") {
            info.is_deprecated = true;
        }

        self.precollect_class_constants(&mut info, &tr.members, Some(name), None);

        if let Some(parsed) = parsed_docblock.as_ref() {
            let template_bindings = self.parse_docblock_template_bindings(
                parsed,
                GenericParent::ClassLike(name),
                Some(name),
                None,
                None,
                Some(&info.constants),
                &mut info.docblock_issues,
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                    variance: binding.variance,
                })
                .collect();
        }

        if let Some(parsed) = parsed_docblock.as_ref() {
            let class_template_map = self.build_template_map_from_class_template_types(
                &info.template_types,
                GenericParent::ClassLike(name),
            );
            // Psalm reads `@psalm-type` aliases from every comment attached
            // to the node (ClassLikeNodeScanner's getComments loop), so
            // collect across the whole preceding docblock run — the closest
            // block is the run's last entry, its aliases winning on collision.
            for run_parsed in self.find_preceding_docblock_run(span.start.offset) {
                class_docblock_type_aliases = self.collect_docblock_type_aliases(
                    &run_parsed,
                    Some(name),
                    None,
                    Some(&class_template_map),
                    Some(&class_docblock_type_aliases),
                );
            }

            for (alias_name, aliased_type) in &class_docblock_type_aliases {
                let scoped_alias = self.interner.intern(&format!(
                    "{}::{}",
                    self.interner.lookup(name),
                    alias_name
                ));
                self.declarations.type_aliases.push(ClassTypeAlias {
                    name: scoped_alias,
                    aliased_type: aliased_type.clone(),
                    file_path: self.file_path,
                    start_offset: span.start.offset,
                });
            }
            self.register_namespace_type_aliases(&class_docblock_type_aliases, span.start.offset);

            let previous_aliases = std::mem::replace(
                &mut self.active_docblock_type_aliases,
                class_docblock_type_aliases.clone(),
            );
            self.apply_docblock_mixins(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_template_extends(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_requirements(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_properties(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.apply_docblock_magic_methods(
                &mut info,
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
            );
            self.active_docblock_type_aliases = previous_aliases;
        }

        // Parse members
        let previous_aliases = std::mem::replace(
            &mut self.active_docblock_type_aliases,
            class_docblock_type_aliases,
        );
        self.collect_class_members(&mut info, &tr.members);
        self.active_docblock_type_aliases = previous_aliases;

        self.declarations.classes.push(info);
    }

    pub(crate) fn visit_enum(&mut self, en: &Enum<'_>) {
        let name = self.make_fqn(en.name.value);
        let span = en.span();

        let enum_backing_atomic = en.backing_type_hint.as_ref().and_then(|backing| {
            self.resolve_type(&backing.hint, Some(name), None)
                .get_single()
                .cloned()
        });

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Enum,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            name_location: {
                let name_span = en.name.span();
                Some((name_span.start.offset, name_span.end.offset))
            },
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(parsed);
            info.specialize_instance = parsed.tags.contains_key("psalm-taint-specialize");
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(parsed);
            info.is_deprecated = self.is_docblock_deprecated(parsed);
            info.deprecation_message = self.get_docblock_deprecation_message(parsed);
            info.internal =
                self.get_docblock_internal_scopes(parsed, info.name, &mut info.docblock_issues);
            info.is_internal = !info.internal.is_empty();
            self.validate_type_alias_docblock_tags(&mut info, parsed, span.start.offset);
        }

        if self.has_attribute_named(&en.attribute_lists, "Deprecated") {
            info.is_deprecated = true;
        }

        info.interfaces.insert(StrId::UNIT_ENUM);
        if let Some(backing_atomic) = enum_backing_atomic.as_ref() {
            info.interfaces.insert(StrId::BACKED_ENUM);
            match backing_atomic {
                TAtomic::TInt => {
                    info.interfaces.insert(StrId::INT_BACKED_ENUM);
                }
                TAtomic::TString => {
                    info.interfaces.insert(StrId::STRING_BACKED_ENUM);
                }
                _ => {}
            }
        }

        // Parse implements
        if let Some(implements) = &en.implements {
            for iface in &implements.types {
                let iface_name = self.resolve_identifier(iface);
                info.interfaces.insert(iface_name);
            }
        }

        self.precollect_class_constants(&mut info, &en.members, Some(name), None);
        self.precollect_enum_case_constants(&mut info, &en.members, enum_backing_atomic.as_ref());
        self.inject_builtin_enum_methods(&mut info, enum_backing_atomic.as_ref());

        if let Some(parsed) = parsed_docblock.as_ref() {
            let template_bindings = self.parse_docblock_template_bindings(
                parsed,
                GenericParent::ClassLike(name),
                Some(name),
                None,
                None,
                Some(&info.constants),
                &mut info.docblock_issues,
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                    variance: binding.variance,
                })
                .collect();
        }

        // `@psalm-type` aliases on enums work like on classes (Psalm's
        // ClassLikeNodeScanner handles every classlike kind).
        let mut class_docblock_type_aliases: FxHashMap<String, TUnion> = FxHashMap::default();
        if parsed_docblock.is_some() {
            let class_template_map = self.build_template_map_from_class_template_types(
                &info.template_types,
                GenericParent::ClassLike(name),
            );
            for run_parsed in self.find_preceding_docblock_run(span.start.offset) {
                class_docblock_type_aliases = self.collect_docblock_type_aliases(
                    &run_parsed,
                    Some(name),
                    None,
                    Some(&class_template_map),
                    Some(&class_docblock_type_aliases),
                );
            }

            for (alias_name, aliased_type) in &class_docblock_type_aliases {
                let scoped_alias = self.interner.intern(&format!(
                    "{}::{}",
                    self.interner.lookup(name),
                    alias_name
                ));
                self.declarations.type_aliases.push(ClassTypeAlias {
                    name: scoped_alias,
                    aliased_type: aliased_type.clone(),
                    file_path: self.file_path,
                    start_offset: span.start.offset,
                });
            }
            self.register_namespace_type_aliases(&class_docblock_type_aliases, span.start.offset);
        }

        // Parse members
        let previous_aliases = std::mem::replace(
            &mut self.active_docblock_type_aliases,
            class_docblock_type_aliases,
        );
        self.collect_class_members(&mut info, &en.members);
        self.active_docblock_type_aliases = previous_aliases;

        self.declarations.classes.push(info);
    }
}
