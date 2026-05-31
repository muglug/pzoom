//! Class-like declaration scanning (class / interface / trait / enum).
//!
//! Mirrors Hakana's `code_info_builder/classlike_scanner.rs`. These methods belong
//! to [`DeclarationCollector`]; split out of the module root for organization.

use mago_span::HasSpan;
use mago_syntax::ast::ast::class_like::{Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::modifier::Modifier;

use pzoom_code_info::class_like_info::{
    ClassLikeInfo, ClassLikeKind, TemplateType,
};
use pzoom_code_info::class_type_alias::ClassTypeAlias;
use pzoom_code_info::{TAtomic, TUnion};
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
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_final = self.is_docblock_final(parsed);
            info.is_immutable = self.is_docblock_immutable(&parsed);
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(&parsed);
            info.is_consistent_constructor = self.is_docblock_consistent_constructor(&parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(&parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(&parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(&parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(&parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(&parsed);
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
        if let Some(extends) = &class.extends {
            if let Some(parent) = extends.types.first() {
                let parent_name = self.resolve_identifier(parent);
                info.parent_class = Some(parent_name);
            }
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
                name,
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
            let class_template_map =
                self.build_template_map_from_class_template_types(&info.template_types, name);
            class_docblock_type_aliases = self.collect_docblock_type_aliases(
                parsed,
                Some(name),
                info.parent_class,
                Some(&class_template_map),
                None,
            );

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
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(&parsed);
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(&parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(&parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(&parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(&parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(&parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(&parsed);
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
                name,
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
            let class_template_map =
                self.build_template_map_from_class_template_types(&info.template_types, name);
            class_docblock_type_aliases = self.collect_docblock_type_aliases(
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
                None,
            );

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
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(&parsed);
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(&parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(&parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(&parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(&parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(&parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(&parsed);
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
                name,
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
            let class_template_map =
                self.build_template_map_from_class_template_types(&info.template_types, name);
            class_docblock_type_aliases = self.collect_docblock_type_aliases(
                parsed,
                Some(name),
                None,
                Some(&class_template_map),
                None,
            );

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

        let enum_backing_atomic = en
            .backing_type_hint
            .as_ref()
            .and_then(|backing| self.resolve_type(&backing.hint, Some(name), None).get_single().cloned());

        let mut info = ClassLikeInfo {
            name,
            kind: ClassLikeKind::Enum,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            ..Default::default()
        };

        let parsed_docblock = self
            .find_preceding_docblock(span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        if let Some(parsed) = parsed_docblock.as_ref() {
            info.is_immutable = self.is_docblock_immutable(&parsed);
            info.is_external_mutation_free =
                info.is_immutable || self.is_docblock_external_mutation_free(&parsed);
            info.no_seal_properties = self.is_docblock_no_seal_properties(&parsed);
            info.override_method_visibility = self.is_docblock_override_method_visibility(&parsed);
            info.override_property_visibility =
                self.is_docblock_override_property_visibility(&parsed);
            info.sealed_properties = self.get_docblock_sealed_properties(&parsed);
            info.sealed_methods = self.get_docblock_sealed_methods(&parsed);
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

        info.interfaces.insert(self.interner.intern("UnitEnum"));
        if let Some(backing_atomic) = enum_backing_atomic.as_ref() {
            info.interfaces.insert(self.interner.intern("BackedEnum"));
            match backing_atomic {
                TAtomic::TInt => {
                    info.interfaces.insert(self.interner.intern("IntBackedEnum"));
                }
                TAtomic::TString => {
                    info.interfaces.insert(self.interner.intern("StringBackedEnum"));
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
                name,
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

        // Parse members
        self.collect_class_members(&mut info, &en.members);

        self.declarations.classes.push(info);
    }
}
