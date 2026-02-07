//! Declaration collector - extracts class, function, and constant declarations from AST.

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::attribute::AttributeList;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::class_like::enum_case::EnumCaseItem;
use mago_syntax::ast::ast::class_like::member::ClassLikeConstantSelector;
use mago_syntax::ast::ast::class_like::member::ClassLikeMember;
use mago_syntax::ast::ast::class_like::property::{Property, PropertyItem};
use mago_syntax::ast::ast::class_like::trait_use::{
    TraitUseAdaptation, TraitUseMethodReference, TraitUseSpecification,
};
use mago_syntax::ast::ast::class_like::{Class, Enum, Interface, Trait};
use mago_syntax::ast::ast::constant::Constant;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::function_like::function::Function;
use mago_syntax::ast::ast::function_like::parameter::FunctionLikeParameter;
use mago_syntax::ast::ast::identifier::Identifier;
use mago_syntax::ast::ast::literal::Literal;
use mago_syntax::ast::ast::modifier::Modifier;
use mago_syntax::ast::ast::namespace::{Namespace, NamespaceBody};
use mago_syntax::ast::ast::type_hint::Hint;
use mago_syntax::ast::ast::r#use::{Use, UseItem, UseItems};
use mago_syntax::ast::sequence::TokenSeparatedSequence;
use mago_syntax::ast::{Program, Sequence, Statement, Trivia, TriviaKind};

use pzoom_code_info::class_like_info::{
    ClassConstantInfo, ClassLikeInfo, ClassLikeKind, DocblockIssue, DuplicatePropertyIssue,
    PropertyInfo, TemplateType, TemplateVariance, TraitMethodAlias, Visibility,
};
use pzoom_code_info::codebase_info::{
    ConstantInfo, InlineCallableParamType, InlineCallableTypeAnnotation, InlineTraceAnnotation,
    InlineTypeAnnotations, InlineVarTypeAnnotation, TypeAliasInfo,
};
use pzoom_code_info::functionlike_info::{
    Assertion, AssertionType, ConditionalReturnCondition, ConditionalReturnType, FunctionLikeInfo,
    FunctionTemplateType, ParamInfo,
};
use pzoom_code_info::{TAtomic, TUnion, combine_union_types};
use pzoom_str::{Interner, StrId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::type_resolver::resolve_hint;

/// Collected declarations from a PHP file.
#[derive(Debug, Default)]
pub struct CollectedDeclarations {
    pub classes: Vec<ClassLikeInfo>,
    pub functions: Vec<FunctionLikeInfo>,
    pub constants: Vec<ConstantInfo>,
    pub type_aliases: Vec<TypeAliasInfo>,
    pub inline_annotations: InlineTypeAnnotations,
}

/// Collects declarations from a parsed PHP program.
pub struct DeclarationCollector<'a, 'p> {
    interner: &'a mut Interner,
    file_path: StrId,
    source: &'p str,
    current_namespace: Option<StrId>,
    use_aliases: FxHashMap<String, StrId>,
    declarations: CollectedDeclarations,
    known_type_aliases: &'a FxHashMap<StrId, TypeAliasInfo>,
    active_docblock_type_aliases: FxHashMap<String, TUnion>,
    /// Trivia (comments) from the program for docblock parsing
    trivia: &'p Sequence<'p, Trivia<'p>>,
}

#[derive(Clone)]
struct DocblockTemplateBinding {
    name: StrId,
    defining_entity: StrId,
    as_type: TUnion,
    variance: TemplateVariance,
}

type TemplateMap = FxHashMap<String, DocblockTemplateBinding>;

impl<'a, 'p> DeclarationCollector<'a, 'p> {
    pub fn new(
        interner: &'a mut Interner,
        file_path: StrId,
        source: &'p str,
        known_type_aliases: &'a FxHashMap<StrId, TypeAliasInfo>,
        trivia: &'p Sequence<'p, Trivia<'p>>,
    ) -> Self {
        Self {
            interner,
            file_path,
            source,
            current_namespace: None,
            use_aliases: FxHashMap::default(),
            declarations: CollectedDeclarations::default(),
            known_type_aliases,
            active_docblock_type_aliases: FxHashMap::default(),
            trivia,
        }
    }

    /// Collect all declarations from a program.
    pub fn collect(mut self, program: &Program<'_>) -> CollectedDeclarations {
        for statement in &program.statements {
            self.visit_statement(statement);
        }

        self.collect_top_level_inline_docblock_annotations(program.statements.as_slice());
        self.collect_inline_trace_annotations_from_source();
        self.declarations
    }

    /// Find the docblock comment that precedes a given position.
    fn find_preceding_docblock(&self, start_offset: u32) -> Option<&'p str> {
        // Find the docblock that ends closest to (but before) the start_offset
        let mut best_match: Option<&'p Trivia<'p>> = None;

        for trivia in self.trivia.iter() {
            if trivia.kind == TriviaKind::DocBlockComment {
                let end = trivia.span.end.offset;
                if end < start_offset {
                    let gap = &self.source[end as usize..start_offset as usize];
                    if !gap.chars().all(char::is_whitespace) {
                        continue;
                    }

                    if best_match
                        .map(|b| trivia.span.end.offset > b.span.end.offset)
                        .unwrap_or(true)
                    {
                        best_match = Some(trivia);
                    }
                }
            }
        }

        best_match.map(|t| t.value)
    }

    fn collect_inline_docblock_annotations_in_span(
        &mut self,
        body_start: u32,
        body_end: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let docblocks: Vec<(u32, &'p str)> = self
            .trivia
            .iter()
            .filter(|trivia| {
                (trivia.kind == TriviaKind::DocBlockComment
                    || trivia.kind == TriviaKind::MultiLineComment)
                    && trivia.value.contains('@')
                    && trivia.span.start.offset >= body_start
                    && trivia.span.end.offset <= body_end
            })
            .map(|trivia| (trivia.span.end.offset, trivia.value))
            .collect();

        for (doc_end, docblock) in docblocks {
            let Some(target_offset) =
                self.find_next_non_whitespace_offset(doc_end.saturating_add(1))
            else {
                continue;
            };

            if target_offset < body_start || target_offset > body_end {
                continue;
            }

            let parsed = crate::docblock::parse(docblock, 0);
            self.collect_inline_var_annotations_from_docblock(
                &parsed,
                target_offset,
                self_class,
                parent_class,
                template_map,
            );
            self.collect_inline_callable_annotations_from_docblock(
                &parsed,
                target_offset,
                self_class,
                parent_class,
                template_map,
            );
            self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);
        }
    }

    fn collect_top_level_inline_docblock_annotations(&mut self, statements: &[Statement<'_>]) {
        let statement_spans: Vec<(u32, u32)> = statements
            .iter()
            .filter_map(|statement| match statement {
                Statement::Expression(_)
                | Statement::Echo(_)
                | Statement::Return(_)
                | Statement::If(_)
                | Statement::While(_)
                | Statement::Foreach(_)
                | Statement::For(_)
                | Statement::Switch(_)
                | Statement::Try(_)
                | Statement::Block(_)
                | Statement::Unset(_)
                | Statement::Noop(_) => {
                    Some((statement.span().start.offset, statement.span().end.offset))
                }
                _ => None,
            })
            .collect();

        let docblocks: Vec<(u32, &'p str)> = self
            .trivia
            .iter()
            .filter(|trivia| {
                (trivia.kind == TriviaKind::DocBlockComment
                    || trivia.kind == TriviaKind::MultiLineComment)
                    && trivia.value.contains('@')
            })
            .map(|trivia| (trivia.span.end.offset, trivia.value))
            .collect();

        for (doc_end, docblock) in docblocks {
            let Some(target_offset) =
                self.find_next_non_whitespace_offset(doc_end.saturating_add(1))
            else {
                continue;
            };

            let parsed = crate::docblock::parse(docblock, 0);
            self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);

            if !statement_spans
                .iter()
                .any(|(start, end)| target_offset >= *start && target_offset <= *end)
            {
                continue;
            }

            self.collect_inline_var_annotations_from_docblock(
                &parsed,
                target_offset,
                None,
                None,
                None,
            );
            self.collect_inline_callable_annotations_from_docblock(
                &parsed,
                target_offset,
                None,
                None,
                None,
            );
        }
    }

    fn collect_inline_var_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(var_tags) = parsed.combined_tags.get("var") else {
            return;
        };

        let mut annotations = Vec::new();

        for content in var_tags.values() {
            let var_name = crate::docblock::extract_var_name_from_content(content)
                .map(|name| self.interner.intern(name));
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                if content.trim().starts_with('$') {
                    annotations.push(InlineVarTypeAnnotation {
                        var_name,
                        var_type: TUnion::mixed(),
                        is_invalid: true,
                    });
                }
                continue;
            };

            let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );
            let is_invalid = !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                None,
            );

            annotations.push(InlineVarTypeAnnotation {
                var_name,
                var_type: resolved_type,
                is_invalid,
            });
        }

        if annotations.is_empty() {
            return;
        }

        self.declarations
            .inline_annotations
            .var_annotations
            .entry(target_offset)
            .or_default()
            .extend(annotations);
    }

    fn collect_inline_callable_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let has_template_annotation = parsed.combined_tags.contains_key("template")
            || parsed.combined_tags.contains_key("template-covariant");
        let is_pure = self.is_docblock_pure(parsed);

        let mut params = Vec::new();
        if let Some(param_tags) = parsed.combined_tags.get("param") {
            for content in param_tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let param_name = crate::docblock::extract_var_name_from_content(content)
                    .map(|name| self.interner.intern(name));

                params.push(InlineCallableParamType {
                    param_name,
                    param_type: resolved_type,
                });
            }
        }

        let return_type = parsed
            .combined_tags
            .get("return")
            .and_then(|tags| {
                tags.values()
                    .next()
                    .and_then(|content| crate::docblock::extract_type_string_from_content(content))
            })
            .map(|type_str| {
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                )
            });

        if params.is_empty() && return_type.is_none() && !has_template_annotation && !is_pure {
            return;
        }

        self.declarations
            .inline_annotations
            .callable_annotations
            .entry(target_offset)
            .and_modify(|existing| {
                existing.params.extend(params.clone());
                if existing.return_type.is_none() {
                    existing.return_type = return_type.clone();
                }
                existing.has_template_annotation |= has_template_annotation;
                existing.is_pure |= is_pure;
            })
            .or_insert_with(|| InlineCallableTypeAnnotation {
                params,
                return_type,
                has_template_annotation,
                is_pure,
            });
    }

    fn collect_inline_trace_annotations_from_docblock(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        target_offset: u32,
    ) {
        let mut trace_annotations = Vec::new();

        for key in ["psalm-trace", "trace"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let var_names = crate::docblock::extract_var_names_from_content(content)
                    .into_iter()
                    .map(|name| self.interner.intern(name))
                    .collect::<Vec<_>>();

                if var_names.is_empty() {
                    continue;
                }

                trace_annotations.push(InlineTraceAnnotation { var_names });
            }
        }

        if trace_annotations.is_empty() {
            return;
        }

        let entry = self
            .declarations
            .inline_annotations
            .trace_annotations
            .entry(target_offset)
            .or_default();

        for annotation in trace_annotations {
            if !entry
                .iter()
                .any(|existing| existing.var_names == annotation.var_names)
            {
                entry.push(annotation);
            }
        }
    }

    fn collect_inline_trace_annotations_from_source(&mut self) {
        let mut cursor = 0usize;

        while let Some(start_rel) = self.source[cursor..].find("/**") {
            let start = cursor + start_rel;
            let comment_start = start + 3;
            let Some(end_rel) = self.source[comment_start..].find("*/") else {
                break;
            };

            let end = comment_start + end_rel + 2;
            let comment = &self.source[start..end];

            if comment.contains("@psalm-trace") || comment.contains("@trace") {
                let parsed = crate::docblock::parse(comment, 0);
                if let Some(target_offset) = self.find_next_non_whitespace_offset(end as u32) {
                    self.collect_inline_trace_annotations_from_docblock(&parsed, target_offset);
                }
            }

            cursor = end;
        }
    }

    fn find_next_non_whitespace_offset(&self, offset: u32) -> Option<u32> {
        let bytes = self.source.as_bytes();
        let mut i = offset as usize;

        while i < bytes.len() {
            match bytes[i] {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    i += 1;
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < bytes.len() && bytes[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < bytes.len() {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                }
                _ => return Some(i as u32),
            }
        }

        None
    }

    fn visit_statement(&mut self, stmt: &Statement<'_>) {
        self.collect_preceding_statement_type_aliases(stmt.span().start.offset);

        match stmt {
            Statement::Namespace(ns) => self.visit_namespace(ns),
            Statement::Use(r#use) => self.visit_use(r#use),
            Statement::Class(class) => self.visit_class(class),
            Statement::Interface(iface) => self.visit_interface(iface),
            Statement::Trait(tr) => self.visit_trait(tr),
            Statement::Enum(en) => self.visit_enum(en),
            Statement::Function(func) => self.visit_function(func),
            Statement::Constant(constant) => self.visit_constant(constant),
            _ => {}
        }
    }

    fn visit_namespace(&mut self, ns: &Namespace<'_>) {
        let previous_namespace = self.current_namespace;
        let previous_use_aliases = std::mem::take(&mut self.use_aliases);

        // Set current namespace
        let ns_name = ns.name.as_ref().map(|n| self.interner.intern(n.value()));
        self.current_namespace = ns_name;

        // Visit statements in namespace
        match &ns.body {
            NamespaceBody::Implicit(implicit) => {
                for stmt in &implicit.statements {
                    self.visit_statement(stmt);
                }
                self.collect_top_level_inline_docblock_annotations(implicit.statements.as_slice());
            }
            NamespaceBody::BraceDelimited(block) => {
                for stmt in &block.statements {
                    self.visit_statement(stmt);
                }
                self.collect_top_level_inline_docblock_annotations(block.statements.as_slice());
            }
        }

        self.current_namespace = previous_namespace;
        self.use_aliases = previous_use_aliases;
    }

    fn visit_use(&mut self, use_stmt: &Use<'_>) {
        match &use_stmt.items {
            UseItems::Sequence(sequence) => {
                for item in &sequence.items {
                    self.register_use_alias(item, None);
                }
            }
            UseItems::TypedSequence(sequence) => {
                if sequence.r#type.is_function() || sequence.r#type.is_const() {
                    return;
                }

                for item in &sequence.items {
                    self.register_use_alias(item, None);
                }
            }
            UseItems::TypedList(list) => {
                if list.r#type.is_function() || list.r#type.is_const() {
                    return;
                }

                let namespace = normalize_use_name(list.namespace.value());
                for item in &list.items {
                    self.register_use_alias(item, Some(namespace.as_str()));
                }
            }
            UseItems::MixedList(list) => {
                let namespace = normalize_use_name(list.namespace.value());
                for item in &list.items {
                    if item
                        .r#type
                        .as_ref()
                        .is_some_and(|t| t.is_function() || t.is_const())
                    {
                        continue;
                    }

                    self.register_use_alias(&item.item, Some(namespace.as_str()));
                }
            }
        }
    }

    fn register_use_alias(&mut self, item: &UseItem<'_>, namespace_prefix: Option<&str>) {
        let item_name = normalize_use_name(item.name.value());
        let full_name = if let Some(prefix) = namespace_prefix {
            format!("{}\\{}", prefix, item_name)
        } else {
            item_name
        };

        let alias = item
            .alias
            .as_ref()
            .map(|a| a.identifier.value.to_string())
            .unwrap_or_else(|| {
                full_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(full_name.as_str())
                    .to_string()
            });

        let alias_key = alias.to_ascii_lowercase();
        let target_id = self.interner.intern(&full_name);
        self.use_aliases.insert(alias_key, target_id);
    }

    fn visit_class(&mut self, class: &Class<'_>) {
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
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
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
                self.declarations.type_aliases.push(TypeAliasInfo {
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

    fn visit_interface(&mut self, iface: &Interface<'_>) {
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
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
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
                self.declarations.type_aliases.push(TypeAliasInfo {
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

    fn visit_trait(&mut self, tr: &Trait<'_>) {
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
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
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
                self.declarations.type_aliases.push(TypeAliasInfo {
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

    fn visit_enum(&mut self, en: &Enum<'_>) {
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
            );
            info.template_types = template_bindings
                .iter()
                .map(|binding| TemplateType {
                    name: binding.name,
                    as_type: binding.as_type.clone(),
                    variance: binding.variance,
                })
                .collect();
        }

        // Parse members
        self.collect_class_members(&mut info, &en.members);

        self.declarations.classes.push(info);
    }

    fn visit_function(&mut self, func: &Function<'_>) {
        let name = self.make_fqn(func.name.value);
        let span = func.span();

        let signature_return_type = func
            .return_type_hint
            .as_ref()
            .map(|rth| self.resolve_type(&rth.hint, None, None));

        let mut params = self.collect_params(&func.parameter_list.parameters, None, None, None);

        let mut return_type = signature_return_type.clone();
        let mut is_pure = false;
        let mut is_mutation_free = false;
        let mut is_deprecated = false;
        let mut deprecation_message = None;
        let mut internal = Vec::new();
        let mut assertions = Vec::new();
        let mut if_true_assertions = Vec::new();
        let mut if_false_assertions = Vec::new();
        let mut template_types = Vec::new();
        let mut conditional_return_type = None;
        let mut if_this_is_type = None;
        let mut inherits_docblock = false;
        let mut no_named_arguments = false;
        let mut function_template_map: TemplateMap = FxHashMap::default();
        let mut function_docblock_issues: Vec<DocblockIssue> = Vec::new();

        if let Some(docblock) = self.find_preceding_docblock(span.start.offset) {
            let parsed = crate::docblock::parse(docblock, 0);
            let template_bindings =
                self.parse_docblock_template_bindings(&parsed, name, None, None, None, None);
            template_types = template_bindings
                .iter()
                .map(|binding| FunctionTemplateType {
                    name: binding.name,
                    as_type: binding.as_type.clone(),
                })
                .collect();

            function_template_map = self.build_template_map_from_bindings(&template_bindings, None);
            self.validate_function_docblock_type_tags(
                &parsed,
                span.start.offset,
                None,
                None,
                Some(&function_template_map),
                None,
                &mut function_docblock_issues,
            );
            inherits_docblock = self.is_docblock_inheritdoc(&parsed);
            is_pure = self.is_docblock_pure(&parsed);
            is_mutation_free = self.is_docblock_mutation_free(&parsed);
            no_named_arguments = self.is_docblock_no_named_arguments(&parsed);
            is_deprecated = self.is_docblock_deprecated(&parsed);
            deprecation_message = self.get_docblock_deprecation_message(&parsed);
            let mut ignored_docblock_issues = Vec::new();
            internal =
                self.get_docblock_internal_scopes(&parsed, name, &mut ignored_docblock_issues);
            let function_docblock_type_aliases = self.collect_docblock_type_aliases(
                &parsed,
                None,
                None,
                Some(&function_template_map),
                None,
            );
            self.register_namespace_type_aliases(
                &function_docblock_type_aliases,
                span.start.offset,
            );

            let previous_aliases = std::mem::replace(
                &mut self.active_docblock_type_aliases,
                function_docblock_type_aliases.clone(),
            );

            self.apply_docblock_param_types(
                &parsed,
                &mut params,
                None,
                None,
                Some(&function_template_map),
                None,
            );
            self.apply_docblock_param_out_types(
                &parsed,
                &mut params,
                None,
                None,
                Some(&function_template_map),
                None,
            );
            if let Some((docblock_return, docblock_conditional_return)) = self
                .get_docblock_return_type(&parsed, None, None, Some(&function_template_map), None)
            {
                return_type = Some(docblock_return);
                conditional_return_type = docblock_conditional_return;
            }
            if_this_is_type = self.get_docblock_if_this_is_type(
                &parsed,
                None,
                None,
                Some(&function_template_map),
                None,
            );

            if let Some(return_type) = return_type.as_mut() {
                if self.is_docblock_ignore_nullable_return(&parsed) {
                    return_type.ignore_nullable_issues = true;
                }
                if self.is_docblock_ignore_falsable_return(&parsed) {
                    return_type.ignore_falsable_issues = true;
                }
            }

            let parsed_assertions =
                self.get_docblock_assertions(&parsed, None, None, Some(&function_template_map));
            assertions.extend(parsed_assertions.assertions);
            if_true_assertions.extend(parsed_assertions.if_true_assertions);
            if_false_assertions.extend(parsed_assertions.if_false_assertions);

            self.active_docblock_type_aliases = previous_aliases;
        }

        let body_span = func.body.span();
        let uses_variadic_builtin_args =
            self.span_contains_variadic_builtin_calls(body_span.start.offset, body_span.end.offset);
        self.collect_inline_docblock_annotations_in_span(
            body_span.start.offset,
            body_span.end.offset,
            None,
            None,
            Some(&function_template_map),
        );

        assertions.extend(self.get_implicit_assertions(
            func.body.statements.as_slice(),
            None,
            None,
        ));
        let defined_constants =
            self.collect_defined_constants_from_statements(func.body.statements.as_slice());
        let has_variadic_param = params.iter().any(|param| param.is_variadic);

        let info = FunctionLikeInfo {
            name,
            params,
            return_type,
            signature_return_type,
            is_pure,
            is_mutation_free,
            is_deprecated: is_deprecated
                || self.has_attribute_named(&func.attribute_lists, "Deprecated"),
            deprecation_message,
            is_internal: !internal.is_empty(),
            internal,
            returns_by_ref: func.ampersand.is_some(),
            is_variadic: uses_variadic_builtin_args || has_variadic_param,
            file_path: self.file_path,
            start_offset: span.start.offset,
            end_offset: span.end.offset,
            assertions,
            if_true_assertions,
            if_false_assertions,
            template_types,
            conditional_return_type,
            if_this_is_type,
            docblock_issues: function_docblock_issues,
            inherits_docblock,
            no_named_arguments,
            defined_constants,
            ..Default::default()
        };

        self.declarations.functions.push(info);
    }

    fn visit_constant(&mut self, constant: &Constant<'_>) {
        for item in &constant.items {
            let name = self.make_fqn(item.name.value);
            let span = item.span();
            let constant_type =
                infer_simple_expression_type(&item.value).unwrap_or_else(TUnion::mixed);

            let info = ConstantInfo {
                name,
                constant_type,
                file_path: self.file_path,
                start_offset: span.start.offset,
            };

            self.declarations.constants.push(info);
        }
    }

    fn precollect_class_constants(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) {
        for member in members {
            let ClassLikeMember::Constant(class_const) = member else {
                continue;
            };

            let visibility = parse_const_visibility(&class_const.modifiers);
            let hinted_const_type = class_const
                .hint
                .as_ref()
                .map(|hint| self.resolve_type(hint, self_class, parent_class));

            for item in &class_const.items {
                let const_name = self.interner.intern(item.name.value);
                let span = item.span();
                let constant_type = hinted_const_type
                    .clone()
                    .or_else(|| infer_simple_expression_type(&item.value))
                    .unwrap_or_else(TUnion::mixed);

                class_info.constants.insert(
                    const_name,
                    ClassConstantInfo {
                        name: const_name,
                        declaring_class: class_info.name,
                        constant_type,
                        visibility,
                        is_final: class_const
                            .modifiers
                            .iter()
                            .any(|modifier| matches!(modifier, Modifier::Final(_))),
                        is_deprecated: false,
                        start_offset: span.start.offset,
                    },
                );
            }
        }
    }

    fn precollect_enum_case_constants(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
        enum_backing_atomic: Option<&TAtomic>,
    ) {
        let mut case_name_types = Vec::new();
        let mut case_value_types = Vec::new();

        for member in members {
            let ClassLikeMember::EnumCase(enum_case) = member else {
                continue;
            };

            let case_name = self.interner.intern(enum_case.item.name().value);
            let case_name_span = enum_case.item.name().span();
            let case_type = TUnion::new(TAtomic::TEnumCase {
                enum_name: class_info.name,
                case_name,
            });

            let case_docblock = self
                .find_preceding_docblock(enum_case.span().start.offset)
                .map(|docblock| crate::docblock::parse(docblock, 0));
            let is_case_deprecated = case_docblock
                .as_ref()
                .is_some_and(|parsed| self.is_docblock_deprecated(parsed))
                || self.has_attribute_named(&enum_case.attribute_lists, "Deprecated");

            class_info.constants.insert(
                case_name,
                ClassConstantInfo {
                    name: case_name,
                    declaring_class: class_info.name,
                    constant_type: case_type,
                    visibility: Visibility::Public,
                    is_final: true,
                    is_deprecated: is_case_deprecated,
                    start_offset: case_name_span.start.offset,
                },
            );

            case_name_types.push(TAtomic::TLiteralString {
                value: enum_case.item.name().value.to_string(),
            });

            if let EnumCaseItem::Backed(backed_case) = &enum_case.item {
                let inferred_case_value =
                    infer_simple_expression_type(&backed_case.value).unwrap_or_else(TUnion::mixed);
                if let Some(single_case_value) = inferred_case_value.get_single() {
                    case_value_types.push(single_case_value.clone());
                } else {
                    case_value_types.push(TAtomic::TMixed);
                }
            }
        }

        if !case_name_types.is_empty() {
            let name_property = self.interner.intern("name");
            class_info.properties.insert(
                name_property,
                PropertyInfo {
                    name: name_property,
                    declaring_class: class_info.name,
                    property_type: Some(TUnion::from_types(case_name_types)),
                    signature_type: None,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_readonly: true,
                    readonly_allow_private_mutation: false,
                    has_default: false,
                    is_promoted: false,
                    is_deprecated: false,
                    internal: Vec::new(),
                    description: None,
                    start_offset: class_info.start_offset,
                },
            );
        }

        if enum_backing_atomic.is_some() && !case_value_types.is_empty() {
            let value_property = self.interner.intern("value");
            class_info.properties.insert(
                value_property,
                PropertyInfo {
                    name: value_property,
                    declaring_class: class_info.name,
                    property_type: Some(TUnion::from_types(case_value_types)),
                    signature_type: None,
                    visibility: Visibility::Public,
                    is_static: false,
                    is_readonly: true,
                    readonly_allow_private_mutation: false,
                    has_default: false,
                    is_promoted: false,
                    is_deprecated: false,
                    internal: Vec::new(),
                    description: None,
                    start_offset: class_info.start_offset,
                },
            );
        }
    }

    fn inject_builtin_enum_methods(
        &mut self,
        class_info: &mut ClassLikeInfo,
        enum_backing_atomic: Option<&TAtomic>,
    ) {
        let enum_case_types: Vec<TAtomic> = class_info
            .constants
            .values()
            .filter_map(|constant| constant.constant_type.get_single().cloned())
            .filter(|atomic| matches!(atomic, TAtomic::TEnumCase { .. }))
            .collect();

        let has_enum_cases = !enum_case_types.is_empty();
        let enum_case_union = if has_enum_cases {
            TUnion::from_types(enum_case_types)
        } else {
            TUnion::new(TAtomic::TNamedObject {
                name: class_info.name,
                type_params: None,
            })
        };

        let cases_return_type = TUnion::new(if has_enum_cases {
            TAtomic::TNonEmptyList {
                value_type: Box::new(enum_case_union),
            }
        } else {
            TAtomic::TList {
                value_type: Box::new(enum_case_union),
            }
        });

        let cases_name = self.interner.intern("cases");
        class_info
            .methods
            .entry(cases_name)
            .or_insert_with(|| FunctionLikeInfo {
                name: cases_name,
                declaring_class: Some(class_info.name),
                params: Vec::new(),
                return_type: Some(cases_return_type.clone()),
                signature_return_type: Some(cases_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            });

        let Some(backing_atomic) = enum_backing_atomic.cloned() else {
            return;
        };

        let value_param = ParamInfo {
            name: self.interner.intern("$value"),
            param_type: Some(TUnion::new(backing_atomic.clone())),
            param_out_type: None,
            signature_type: Some(TUnion::new(backing_atomic)),
            has_docblock_type: false,
            is_optional: false,
            is_variadic: false,
            by_ref: false,
            is_promoted: false,
            default_type: None,
            description: None,
            start_offset: class_info.start_offset,
        };

        let from_name = self.interner.intern("from");
        let from_return_type = TUnion::new(TAtomic::TNamedObject {
            name: class_info.name,
            type_params: None,
        });
        class_info
            .methods
            .entry(from_name)
            .or_insert_with(|| FunctionLikeInfo {
                name: from_name,
                declaring_class: Some(class_info.name),
                params: vec![value_param.clone()],
                return_type: Some(from_return_type.clone()),
                signature_return_type: Some(from_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            });

        let try_from_name = self.interner.intern("tryFrom");
        let mut try_from_return_type = TUnion::new(TAtomic::TNamedObject {
            name: class_info.name,
            type_params: None,
        });
        try_from_return_type.add_type(TAtomic::TNull);
        class_info
            .methods
            .entry(try_from_name)
            .or_insert_with(|| FunctionLikeInfo {
                name: try_from_name,
                declaring_class: Some(class_info.name),
                params: vec![value_param],
                return_type: Some(try_from_return_type.clone()),
                signature_return_type: Some(try_from_return_type),
                is_pure: true,
                is_mutation_free: true,
                is_static: true,
                visibility: Visibility::Public,
                file_path: self.file_path,
                start_offset: class_info.start_offset,
                end_offset: class_info.end_offset,
                ..Default::default()
            });
    }

    fn collect_class_members(
        &mut self,
        class_info: &mut ClassLikeInfo,
        members: &Sequence<'_, ClassLikeMember<'_>>,
    ) {
        let class_template_map = self.build_template_map_from_class_template_types(
            &class_info.template_types,
            class_info.name,
        );
        let member_self_class = if class_info.kind == ClassLikeKind::Trait {
            None
        } else {
            Some(class_info.name)
        };

        for member in members {
            match member {
                ClassLikeMember::Method(method) => {
                    let method_name = self.interner.intern(method.name.value);
                    let span = method.span();

                    let signature_return_type = method.return_type_hint.as_ref().map(|rth| {
                        self.resolve_type(&rth.hint, member_self_class, class_info.parent_class)
                    });

                    let mut params = self.collect_params(
                        &method.parameter_list.parameters,
                        member_self_class,
                        class_info.parent_class,
                        Some(&class_info.constants),
                    );
                    let mut return_type = signature_return_type.clone();
                    let mut is_pure = false;
                    let mut is_mutation_free = false;
                    let mut is_deprecated = false;
                    let mut deprecation_message = None;
                    let mut internal = Vec::new();
                    let mut assertions = Vec::new();
                    let mut if_true_assertions = Vec::new();
                    let mut if_false_assertions = Vec::new();
                    let mut template_types = Vec::new();
                    let mut conditional_return_type = None;
                    let mut if_this_is_type = None;
                    let mut inherits_docblock = false;
                    let mut no_named_arguments = false;
                    let mut method_template_map = class_template_map.clone();
                    let mut method_docblock_issues: Vec<DocblockIssue> = Vec::new();

                    if let Some(docblock) = self.find_preceding_docblock(span.start.offset) {
                        let parsed = crate::docblock::parse(docblock, 0);
                        inherits_docblock = self.is_docblock_inheritdoc(&parsed);
                        is_pure = self.is_docblock_pure(&parsed);
                        is_mutation_free = self.is_docblock_mutation_free(&parsed);
                        no_named_arguments = self.is_docblock_no_named_arguments(&parsed);
                        is_deprecated = self.is_docblock_deprecated(&parsed);
                        deprecation_message = self.get_docblock_deprecation_message(&parsed);
                        internal = self.get_docblock_internal_scopes(
                            &parsed,
                            class_info.name,
                            &mut class_info.docblock_issues,
                        );

                        let method_defining_entity = self.interner.intern(&format!(
                            "{}::{}",
                            self.interner.lookup(class_info.name),
                            self.interner.lookup(method_name)
                        ));
                        let method_template_bindings = self.parse_docblock_template_bindings(
                            &parsed,
                            method_defining_entity,
                            member_self_class,
                            class_info.parent_class,
                            Some(&class_template_map),
                            Some(&class_info.constants),
                        );
                        template_types = method_template_bindings
                            .iter()
                            .map(|binding| FunctionTemplateType {
                                name: binding.name,
                                as_type: binding.as_type.clone(),
                            })
                            .collect();
                        method_template_map = self.build_template_map_from_bindings(
                            &method_template_bindings,
                            Some(&class_template_map),
                        );
                        self.validate_function_docblock_type_tags(
                            &parsed,
                            span.start.offset,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                            &mut method_docblock_issues,
                        );

                        self.apply_docblock_param_types(
                            &parsed,
                            &mut params,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );
                        self.apply_docblock_param_out_types(
                            &parsed,
                            &mut params,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );
                        if let Some((docblock_return, docblock_conditional_return)) = self
                            .get_docblock_return_type(
                                &parsed,
                                member_self_class,
                                class_info.parent_class,
                                Some(&method_template_map),
                                Some(&class_info.constants),
                            )
                        {
                            return_type = Some(docblock_return);
                            conditional_return_type = docblock_conditional_return;
                        }
                        if_this_is_type = self.get_docblock_if_this_is_type(
                            &parsed,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                            Some(&class_info.constants),
                        );

                        if let Some(return_type) = return_type.as_mut() {
                            if self.is_docblock_ignore_nullable_return(&parsed) {
                                return_type.ignore_nullable_issues = true;
                            }
                            if self.is_docblock_ignore_falsable_return(&parsed) {
                                return_type.ignore_falsable_issues = true;
                            }
                        }

                        let parsed_assertions = self.get_docblock_assertions(
                            &parsed,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                        );
                        assertions.extend(parsed_assertions.assertions);
                        if_true_assertions.extend(parsed_assertions.if_true_assertions);
                        if_false_assertions.extend(parsed_assertions.if_false_assertions);
                    }

                    let mut uses_variadic_builtin_args = false;
                    if let mago_syntax::ast::ast::class_like::method::MethodBody::Concrete(body) =
                        &method.body
                    {
                        let body_span = body.span();
                        uses_variadic_builtin_args = self.span_contains_variadic_builtin_calls(
                            body_span.start.offset,
                            body_span.end.offset,
                        );
                        self.collect_inline_docblock_annotations_in_span(
                            body_span.start.offset,
                            body_span.end.offset,
                            member_self_class,
                            class_info.parent_class,
                            Some(&method_template_map),
                        );

                        assertions.extend(self.get_implicit_assertions(
                            body.statements.as_slice(),
                            member_self_class,
                            class_info.parent_class,
                        ));
                    }

                    let (visibility, is_static, is_abstract, is_final) =
                        parse_method_modifiers(&method.modifiers);

                    if method_name == StrId::CONSTRUCT {
                        self.collect_promoted_properties(
                            class_info,
                            &method.parameter_list.parameters,
                            &params,
                        );
                    }
                    let has_variadic_param = params.iter().any(|param| param.is_variadic);

                    let method_info = FunctionLikeInfo {
                        name: method_name,
                        declaring_class: Some(class_info.name),
                        params,
                        return_type,
                        signature_return_type,
                        is_pure,
                        is_mutation_free,
                        is_deprecated: is_deprecated
                            || self.has_attribute_named(&method.attribute_lists, "Deprecated"),
                        deprecation_message,
                        is_internal: !internal.is_empty(),
                        internal,
                        is_static,
                        is_abstract,
                        is_final,
                        visibility,
                        returns_by_ref: method.ampersand.is_some(),
                        is_variadic: uses_variadic_builtin_args || has_variadic_param,
                        file_path: self.file_path,
                        start_offset: span.start.offset,
                        end_offset: span.end.offset,
                        assertions,
                        if_true_assertions,
                        if_false_assertions,
                        template_types,
                        conditional_return_type,
                        if_this_is_type,
                        inherits_docblock,
                        no_named_arguments,
                        docblock_issues: method_docblock_issues,
                        ..Default::default()
                    };

                    class_info.methods.insert(method_name, method_info);
                }
                ClassLikeMember::Property(property) => {
                    self.collect_property(class_info, property);
                }
                ClassLikeMember::Constant(class_const) => {
                    let visibility = parse_const_visibility(&class_const.modifiers);
                    let const_docblock = self
                        .find_preceding_docblock(class_const.span().start.offset)
                        .map(|docblock| crate::docblock::parse(docblock, 0));
                    let is_const_deprecated = const_docblock
                        .as_ref()
                        .is_some_and(|parsed| self.is_docblock_deprecated(parsed))
                        || self.has_attribute_named(&class_const.attribute_lists, "Deprecated");

                    let hinted_const_type = class_const.hint.as_ref().map(|h| {
                        self.resolve_type(h, Some(class_info.name), class_info.parent_class)
                    });

                    for item in &class_const.items {
                        let const_name = self.interner.intern(item.name.value);
                        let span = item.span();
                        let inferred_const_type = hinted_const_type
                            .clone()
                            .or_else(|| infer_simple_expression_type(&item.value))
                            .unwrap_or_else(TUnion::mixed);

                        let const_info = ClassConstantInfo {
                            name: const_name,
                            declaring_class: class_info.name,
                            constant_type: inferred_const_type,
                            visibility,
                            is_final: class_const
                                .modifiers
                                .iter()
                                .any(|m| matches!(m, Modifier::Final(_))),
                            is_deprecated: is_const_deprecated,
                            start_offset: span.start.offset,
                        };

                        class_info.constants.insert(const_name, const_info);
                    }
                }
                ClassLikeMember::TraitUse(trait_use) => {
                    for trait_name in &trait_use.trait_names {
                        let name = self.resolve_identifier(trait_name);
                        class_info.used_traits.insert(name);
                    }

                    if let Some(docblock) =
                        self.find_preceding_docblock(trait_use.span().start.offset)
                    {
                        let parsed = crate::docblock::parse(docblock, 0);
                        if let Some(use_tags) = parsed.combined_tags.get("use") {
                            for content in use_tags.values() {
                                let parsed_type =
                                    crate::docblock::parse_type_string(content, self.interner);
                                let resolved_type = self.resolve_docblock_union_type(
                                    parsed_type,
                                    Some(class_info.name),
                                    class_info.parent_class,
                                    Some(&class_template_map),
                                );

                                for atomic in resolved_type.types {
                                    if let TAtomic::TNamedObject {
                                        name,
                                        type_params: Some(type_params),
                                    } = atomic
                                    {
                                        class_info
                                            .template_extended_offsets
                                            .insert(name, type_params);
                                    }
                                }
                            }
                        }
                    }

                    if let TraitUseSpecification::Concrete(specification) = &trait_use.specification
                    {
                        for adaptation in &specification.adaptations {
                            if let TraitUseAdaptation::Alias(alias_adaptation) = adaptation {
                                let (trait_name, original_name) =
                                    match &alias_adaptation.method_reference {
                                        TraitUseMethodReference::Identifier(method_name) => {
                                            (None, self.interner.intern(method_name.value))
                                        }
                                        TraitUseMethodReference::Absolute(method_ref) => (
                                            Some(self.resolve_identifier(&method_ref.trait_name)),
                                            self.interner.intern(method_ref.method_name.value),
                                        ),
                                    };

                                let alias_name = alias_adaptation
                                    .alias
                                    .as_ref()
                                    .map(|a| self.interner.intern(a.value))
                                    .unwrap_or(original_name);

                                let visibility = alias_adaptation
                                    .visibility
                                    .as_ref()
                                    .and_then(parse_visibility_modifier);

                                class_info.trait_method_aliases.push(TraitMethodAlias {
                                    trait_name,
                                    original_name,
                                    alias_name,
                                    visibility,
                                });
                            }
                        }
                    }
                }
                ClassLikeMember::EnumCase(_) => {
                    // Enum cases are handled differently
                }
            }
        }
    }

    fn apply_docblock_magic_properties(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        for tag_name in ["property", "property-read", "property-write"] {
            let Some(tags) = parsed.combined_tags.get(tag_name) else {
                continue;
            };

            let mut ordered_tags: Vec<_> = tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (_, content) in ordered_tags {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    self.push_docblock_issue(
                        class_info,
                        "Badly-formatted @property annotation".to_string(),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };
                let Some(var_name) = crate::docblock::extract_var_name_from_content(content) else {
                    self.push_docblock_issue(
                        class_info,
                        "Badly-formatted @property name".to_string(),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };

                let prop_name = self.interner.intern(var_name.trim_start_matches('$'));
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let resolved_type = self.expand_docblock_class_constant_wildcards(
                    resolved_type,
                    self_class,
                    parent_class,
                    Some(&class_info.constants),
                );

                if tag_name != "property-write" {
                    class_info
                        .pseudo_property_get_types
                        .insert(prop_name, resolved_type.clone());
                }

                if tag_name != "property-read" {
                    class_info
                        .pseudo_property_set_types
                        .insert(prop_name, resolved_type);
                }
            }
        }
    }

    fn apply_docblock_requirements(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        self.collect_required_classlikes_from_docblock_tags(
            class_info,
            parsed,
            &["psalm-require-extends", "require-extends"],
            self_class,
            parent_class,
            template_map,
            true,
        );
        self.collect_required_classlikes_from_docblock_tags(
            class_info,
            parsed,
            &["psalm-require-implements", "require-implements"],
            self_class,
            parent_class,
            template_map,
            false,
        );
    }

    fn collect_required_classlikes_from_docblock_tags(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        tag_keys: &[&str],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        is_extends_requirement: bool,
    ) {
        for tag_key in tag_keys {
            let Some(tags) = parsed.tags.get(*tag_key) else {
                continue;
            };

            let mut ordered_tags: Vec<_> = tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (_, content) in ordered_tags {
                let requirement = take_first_docblock_type_token(content.trim());
                if requirement.is_empty() {
                    self.push_docblock_issue(
                        class_info,
                        format!("{tag_key} annotation used without specifying class-like"),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                }

                let parsed_type = crate::docblock::parse_type_string(requirement, self.interner);
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                let Some(required_classlike) =
                    resolved_type.types.iter().find_map(|atomic| match atomic {
                        TAtomic::TNamedObject { name, .. } => Some(*name),
                        _ => None,
                    })
                else {
                    self.push_docblock_issue(
                        class_info,
                        format!("Badly-formatted {tag_key} annotation"),
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                };

                let target = if is_extends_requirement {
                    &mut class_info.required_extends
                } else {
                    &mut class_info.required_implements
                };

                if !target.contains(&required_classlike) {
                    target.push(required_classlike);
                }
            }
        }
    }

    fn apply_docblock_mixins(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(tags) = parsed.combined_tags.get("mixin") else {
            return;
        };

        if class_info.mixin_declaring_class.is_none() {
            class_info.mixin_declaring_class = Some(class_info.name);
        }

        let mut ordered_tags: Vec<_> = tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        for (_, content) in ordered_tags {
            let mixin = take_first_docblock_type_token(content.trim());
            if mixin.is_empty() {
                self.push_docblock_issue(
                    class_info,
                    "@mixin annotation used without specifying class".to_string(),
                    class_info.start_offset,
                    class_info.start_offset.saturating_add(1),
                );
                continue;
            }

            let parsed_type = crate::docblock::parse_type_string(mixin, self.interner);
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );

            for atomic in resolved_type.types {
                if !class_info.named_mixins.contains(&atomic) {
                    class_info.named_mixins.push(atomic);
                }
            }
        }
    }

    fn apply_docblock_magic_methods(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        let Some(tags) = parsed.combined_tags.get("method") else {
            return;
        };

        if class_info.sealed_methods.is_none() {
            class_info.sealed_methods = Some(true);
        }

        let mut ordered_tags: Vec<_> = tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        for (_, content) in ordered_tags {
            let method_info = match self.parse_docblock_method_info(
                class_info,
                content,
                self_class,
                parent_class,
                template_map,
            ) {
                Ok(method_info) => method_info,
                Err(message) => {
                    self.push_docblock_issue(
                        class_info,
                        message,
                        class_info.start_offset,
                        class_info.start_offset.saturating_add(1),
                    );
                    continue;
                }
            };

            if method_info.is_static {
                class_info
                    .pseudo_static_methods
                    .entry(method_info.name)
                    .or_insert(method_info);
            } else {
                class_info
                    .pseudo_methods
                    .entry(method_info.name)
                    .or_insert(method_info);
            }
        }
    }

    fn apply_docblock_template_extends(
        &mut self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        for tag_name in ["extends", "implements", "use"] {
            let Some(tags) = parsed.combined_tags.get(tag_name) else {
                continue;
            };

            for content in tags.values() {
                let parsed_type = crate::docblock::parse_type_string(content, self.interner);
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                for atomic in resolved_type.types {
                    if let TAtomic::TNamedObject {
                        name,
                        type_params: Some(type_params),
                    } = atomic
                    {
                        class_info
                            .template_extended_offsets
                            .insert(name, type_params);
                    }
                }
            }
        }
    }

    fn parse_docblock_method_info(
        &mut self,
        class_info: &ClassLikeInfo,
        content: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Result<FunctionLikeInfo, String> {
        let mut signature = content.trim();
        if signature.is_empty() {
            return Err("No @method entry specified".to_string());
        }

        let mut is_static = false;
        if let Some(rest) = signature.strip_prefix("static ") {
            is_static = true;
            signature = rest.trim();
        }

        let (open_paren, close_paren) = find_docblock_method_signature_bounds(signature)
            .ok_or_else(|| format!("{signature} is not a valid method"))?;

        let before_paren = signature[..open_paren].trim();
        let params_str = signature[open_paren + 1..close_paren].trim();
        if before_paren.is_empty() {
            return Err(format!("{signature} is not a valid method"));
        }

        let (before_return, method_name) = split_method_name(before_paren)
            .ok_or_else(|| format!("{signature} is not a valid method"))?;

        if !is_valid_docblock_method_name(method_name) {
            return Err(format!("{signature} is not a valid method"));
        }

        let mut return_type_str = if before_return.is_empty() {
            None
        } else {
            Some(before_return.to_string())
        };

        if return_type_str.is_none() {
            let after = signature[close_paren + 1..].trim_start();
            if let Some(return_fragment) = after.strip_prefix(':') {
                let type_fragment = take_first_docblock_type_token(return_fragment.trim_start());
                if !type_fragment.is_empty() {
                    return_type_str = Some(type_fragment.to_string());
                }
            }
        }

        let return_type = if let Some(return_type_str) = return_type_str {
            let parsed_type =
                crate::docblock::parse_type_string(return_type_str.trim(), self.interner);
            Some(self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            ))
        } else {
            Some(TUnion::mixed())
        };

        let params =
            self.parse_docblock_method_params(params_str, self_class, parent_class, template_map)?;

        Ok(FunctionLikeInfo {
            name: self.interner.intern(method_name),
            declaring_class: Some(class_info.name),
            params,
            return_type,
            signature_return_type: None,
            is_pure: false,
            is_mutation_free: false,
            is_static,
            is_abstract: false,
            is_final: false,
            visibility: Visibility::Public,
            returns_by_ref: false,
            file_path: class_info.file_path,
            start_offset: class_info.start_offset,
            end_offset: class_info.end_offset,
            assertions: Vec::new(),
            if_true_assertions: Vec::new(),
            if_false_assertions: Vec::new(),
            template_types: Vec::new(),
            ..Default::default()
        })
    }

    fn parse_docblock_method_params(
        &mut self,
        params: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Result<Vec<ParamInfo>, String> {
        if params.trim().is_empty() {
            return Ok(Vec::new());
        }

        let parsed = split_docblock_method_params(params)
            .into_iter()
            .enumerate()
            .map(|(idx, raw_param)| {
                let raw_param = raw_param.trim();
                if raw_param.is_empty() {
                    return Ok(None);
                }

                if raw_param.contains("& $") {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                let Some(param_name_raw) = extract_param_name_from_content(raw_param) else {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                };

                let param_name_str = format!("${param_name_raw}");
                let Some(param_name_offset) = raw_param.find(&param_name_str) else {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                };

                let mut before_name = raw_param[..param_name_offset].trim_end();
                let after_name = raw_param[param_name_offset + param_name_str.len()..].trim_start();

                let mut is_variadic = false;
                let mut by_ref = false;

                loop {
                    let trimmed = before_name.trim_end();

                    if let Some(stripped) = trimmed.strip_suffix("...") {
                        is_variadic = true;
                        before_name = stripped;
                        continue;
                    }

                    if let Some(stripped) = trimmed.strip_suffix('&') {
                        by_ref = true;
                        before_name = stripped;
                        continue;
                    }

                    break;
                }

                let type_source = before_name.trim();

                if type_source.contains('&')
                    || type_source.contains("...")
                    || after_name.contains("&$")
                    || after_name.contains("& $")
                {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                let type_union = if type_source.is_empty() {
                    None
                } else {
                    let parsed_type =
                        crate::docblock::parse_type_string(type_source, self.interner);
                    Some(self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    ))
                };

                let param_name = self.interner.intern(&param_name_str);
                let is_optional = after_name.contains('=');

                if by_ref && type_union.is_none() {
                    return Err(format!("Badly-formatted @method parameter {raw_param}"));
                }

                Ok(Some(ParamInfo {
                    name: param_name,
                    param_type: Some(type_union.unwrap_or_else(TUnion::mixed)),
                    param_out_type: None,
                    signature_type: None,
                    has_docblock_type: true,
                    is_optional,
                    is_variadic,
                    by_ref,
                    is_promoted: false,
                    default_type: None,
                    description: None,
                    start_offset: idx as u32,
                }))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(parsed.into_iter().flatten().collect())
    }

    fn collect_property(&mut self, class_info: &mut ClassLikeInfo, property: &Property<'_>) {
        let (visibility, is_static, mut is_readonly) =
            parse_property_modifiers(property.modifiers());
        let class_template_map = self.build_template_map_from_class_template_types(
            &class_info.template_types,
            class_info.name,
        );

        // Get native PHP type hint (signature_type)
        let signature_type = property
            .hint()
            .map(|h| self.resolve_type(h, Some(class_info.name), class_info.parent_class));

        // Get property start offset for docblock lookup
        let prop_span = property.span();

        // Get docblock type/flags if present
        let parsed_docblock = self
            .find_preceding_docblock(prop_span.start.offset)
            .map(|docblock| crate::docblock::parse(docblock, 0));

        let property_attribute_lists = match property {
            Property::Plain(plain) => &plain.attribute_lists,
            Property::Hooked(hooked) => &hooked.attribute_lists,
        };
        let mut is_deprecated = self.has_attribute_named(property_attribute_lists, "Deprecated");
        let mut internal = Vec::new();

        if let Some(parsed) = parsed_docblock.as_ref() {
            self.validate_property_docblock_tags(class_info, parsed, prop_span.start.offset);
            is_deprecated |= self.is_docblock_deprecated(parsed);
            internal = self.get_docblock_internal_scopes(
                parsed,
                class_info.name,
                &mut class_info.docblock_issues,
            );
        }

        let mut docblock_type = None;
        if let Some(parsed) = parsed_docblock.as_ref()
            && let Some(var_content) = parsed.get_var()
            && let Some(type_str) = crate::docblock::extract_type_string_from_content(var_content)
        {
            if self.is_valid_docblock_type_string(
                type_str,
                Some(class_info.name),
                class_info.parent_class,
                Some(&class_template_map),
                None,
            ) {
                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                docblock_type = Some(self.resolve_docblock_union_type(
                    parsed_type,
                    Some(class_info.name),
                    class_info.parent_class,
                    Some(&class_template_map),
                ));
            } else {
                self.push_docblock_issue(
                    class_info,
                    "Invalid docblock type".to_string(),
                    prop_span.start.offset,
                    prop_span.start.offset.saturating_add(1),
                );
            }
        }

        let mut readonly_allow_private_mutation = false;
        if let Some(parsed) = parsed_docblock.as_ref() {
            if self.is_docblock_readonly(parsed) {
                is_readonly = true;
            }
            readonly_allow_private_mutation =
                self.is_docblock_readonly_allow_private_mutation(parsed);
        }

        // Match Psalm/Hakana: docblock property types are the effective analysis types.
        // Native signatures remain available via `signature_type`.
        let property_type = docblock_type.clone().or(signature_type.clone());

        match property {
            Property::Plain(plain) => {
                for item in &plain.items {
                    self.add_property_item(
                        class_info,
                        item,
                        property_type.clone(),
                        signature_type.clone(),
                        visibility,
                        is_static,
                        is_readonly,
                        readonly_allow_private_mutation,
                        is_deprecated,
                        internal.clone(),
                    );
                }
            }
            Property::Hooked(hooked) => {
                self.add_property_item(
                    class_info,
                    &hooked.item,
                    property_type.clone(),
                    signature_type.clone(),
                    visibility,
                    is_static,
                    is_readonly,
                    readonly_allow_private_mutation,
                    is_deprecated,
                    internal,
                );
            }
        }
    }

    fn add_property_item(
        &mut self,
        class_info: &mut ClassLikeInfo,
        item: &PropertyItem<'_>,
        property_type: Option<TUnion>,
        signature_type: Option<TUnion>,
        visibility: Visibility,
        is_static: bool,
        is_readonly: bool,
        readonly_allow_private_mutation: bool,
        is_deprecated: bool,
        internal: Vec<StrId>,
    ) {
        let variable = item.variable();
        // Strip the leading $ from property names to match how they're referenced
        let prop_name_str = variable.name.strip_prefix('$').unwrap_or(variable.name);
        let prop_name = self.interner.intern(prop_name_str);
        let span = item.span();
        let has_default = matches!(item, PropertyItem::Concrete(_));

        let prop_info = PropertyInfo {
            name: prop_name,
            declaring_class: class_info.name,
            property_type,
            signature_type,
            visibility,
            is_static,
            is_readonly,
            readonly_allow_private_mutation,
            has_default,
            is_promoted: false,
            is_deprecated,
            internal,
            description: None,
            start_offset: span.start.offset,
        };

        if class_info.properties.contains_key(&prop_name) {
            class_info
                .duplicate_property_issues
                .push(DuplicatePropertyIssue {
                    property_name: prop_name,
                    start_offset: span.start.offset,
                    end_offset: span.end.offset,
                });
        }

        class_info.properties.insert(prop_name, prop_info);
    }

    fn collect_params(
        &mut self,
        params: &TokenSeparatedSequence<'_, FunctionLikeParameter<'_>>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Vec<ParamInfo> {
        params
            .iter()
            .map(|param| {
                let name = self.interner.intern(param.variable.name);
                // Native PHP type hint is the signature_type
                let mut signature_type = param
                    .hint
                    .as_ref()
                    .map(|h| self.resolve_type(h, self_class, parent_class));
                let default_type = param.default_value.as_ref().and_then(|default_value| {
                    infer_param_default_type(
                        &default_value.value,
                        self.interner,
                        self_class,
                        class_constants,
                    )
                });

                // Legacy PHP signatures like `A $a = null` are nullable at runtime.
                if default_type.as_ref().is_some_and(TUnion::is_null) {
                    if let Some(signature_type) = signature_type.as_mut() {
                        if !signature_type.is_nullable {
                            signature_type.add_type(TAtomic::TNull);
                            signature_type.is_nullable = true;
                        }
                    }
                }

                // For now, param_type is same as signature_type
                // Docblock param types will be resolved during analysis
                let param_type = signature_type.clone();

                ParamInfo {
                    name,
                    param_type,
                    param_out_type: None,
                    signature_type,
                    has_docblock_type: false,
                    is_optional: param.default_value.is_some(),
                    is_variadic: param.ellipsis.is_some(),
                    by_ref: param.ampersand.is_some(),
                    is_promoted: param.is_promoted_property(),
                    default_type,
                    description: None,
                    start_offset: param.span().start.offset,
                }
            })
            .collect()
    }

    fn collect_promoted_properties(
        &mut self,
        class_info: &mut ClassLikeInfo,
        ast_params: &TokenSeparatedSequence<'_, FunctionLikeParameter<'_>>,
        params: &[ParamInfo],
    ) {
        for (ast_param, param_info) in ast_params.iter().zip(params.iter()) {
            if !ast_param.is_promoted_property() {
                continue;
            }

            let (visibility, is_static, is_readonly) =
                parse_property_modifiers(&ast_param.modifiers);
            let prop_name_str = ast_param
                .variable
                .name
                .strip_prefix('$')
                .unwrap_or(ast_param.variable.name);
            let prop_name = self.interner.intern(prop_name_str);

            if class_info.properties.contains_key(&prop_name) {
                continue;
            }

            let span = ast_param.span();
            let prop_info = PropertyInfo {
                name: prop_name,
                declaring_class: class_info.name,
                property_type: param_info
                    .param_type
                    .clone()
                    .or_else(|| param_info.signature_type.clone()),
                signature_type: param_info.signature_type.clone(),
                visibility,
                is_static,
                is_readonly,
                readonly_allow_private_mutation: false,
                has_default: ast_param.default_value.is_some(),
                is_promoted: true,
                is_deprecated: false,
                internal: Vec::new(),
                description: None,
                start_offset: span.start.offset,
            };

            class_info.properties.insert(prop_name, prop_info);
        }
    }

    fn apply_docblock_param_out_types(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        params: &mut [ParamInfo],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) {
        let Some(param_out_tags) = parsed.combined_tags.get("param-out") else {
            return;
        };

        let mut ordered_tags: Vec<_> = param_out_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        let mut parsed_tags: Vec<(Option<String>, TUnion)> = Vec::with_capacity(ordered_tags.len());
        for (_, content) in ordered_tags {
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                continue;
            };

            if !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                class_constants,
            ) {
                continue;
            }

            let parsed_type = self
                .try_resolve_docblock_utility_type(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                )
                .unwrap_or_else(|| {
                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                    self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    )
                });
            let param_name = extract_param_name_from_content(content).map(str::to_string);
            parsed_tags.push((param_name, parsed_type));
        }

        if parsed_tags.is_empty() {
            return;
        }

        let use_positional_fallback =
            parsed_tags.len() == params.len() && parsed_tags.iter().all(|(name, _)| name.is_none());

        for (idx, param) in params.iter_mut().enumerate() {
            let param_name = self.interner.lookup(param.name);
            let normalized_name = param_name
                .as_ref()
                .strip_prefix('$')
                .unwrap_or(param_name.as_ref());

            let docblock_type = parsed_tags
                .iter()
                .find_map(|(name, ty)| {
                    if name
                        .as_deref()
                        .map(|name| name.trim_start_matches('$'))
                        == Some(normalized_name)
                    {
                        Some(ty.clone())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if use_positional_fallback {
                        parsed_tags.get(idx).map(|(_, ty)| ty.clone())
                    } else {
                        None
                    }
                });

            if let Some(docblock_type) = docblock_type {
                param.param_out_type = Some(docblock_type);
            }
        }
    }

    fn apply_docblock_param_types(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        params: &mut [ParamInfo],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) {
        let Some(param_tags) = parsed.combined_tags.get("param") else {
            return;
        };

        let mut ordered_tags: Vec<_> = param_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        let mut parsed_tags: Vec<(Option<String>, TUnion)> = Vec::with_capacity(ordered_tags.len());
        for (_, content) in ordered_tags {
            let Some(type_str) = crate::docblock::extract_type_string_from_content(content) else {
                continue;
            };

            if !self.is_valid_docblock_type_string(
                type_str,
                self_class,
                parent_class,
                template_map,
                class_constants,
            ) {
                continue;
            }

            let parsed_type = self
                .try_resolve_docblock_utility_type(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                )
                .unwrap_or_else(|| {
                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                    self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    )
                });
            let param_name = extract_param_name_from_content(content).map(str::to_string);
            parsed_tags.push((param_name, parsed_type));
        }

        if parsed_tags.is_empty() {
            return;
        }

        let use_positional_fallback =
            parsed_tags.len() == params.len() && parsed_tags.iter().all(|(name, _)| name.is_none());

        for (idx, param) in params.iter_mut().enumerate() {
            let param_name = self.interner.lookup(param.name);
            let normalized_name = param_name
                .as_ref()
                .strip_prefix('$')
                .unwrap_or(param_name.as_ref());

            let docblock_type = parsed_tags
                .iter()
                .find_map(|(name, ty)| {
                    if name
                        .as_deref()
                        .map(|name| name.trim_start_matches('$'))
                        == Some(normalized_name)
                    {
                        Some(ty.clone())
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    if use_positional_fallback {
                        parsed_tags.get(idx).map(|(_, ty)| ty.clone())
                    } else {
                        None
                    }
                });

            if let Some(docblock_type) = docblock_type {
                param.param_type = Some(docblock_type);
                param.has_docblock_type = true;
            }
        }
    }

    fn try_resolve_template_key_of_type(
        &self,
        type_str: &str,
        template_map: Option<&TemplateMap>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();
        let inner = trimmed.strip_prefix("key-of<")?.strip_suffix('>')?.trim();
        let template_binding = template_map.and_then(|map| map.get(inner))?;

        Some(resolve_key_of_template_union(&template_binding.as_type))
    }

    fn try_resolve_template_value_of_type(
        &self,
        type_str: &str,
        template_map: Option<&TemplateMap>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();
        let inner = trimmed.strip_prefix("value-of<")?.strip_suffix('>')?.trim();
        let template_binding = template_map.and_then(|map| map.get(inner))?;

        Some(resolve_value_of_template_union(&template_binding.as_type))
    }

    fn try_resolve_docblock_utility_type(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        let trimmed = type_str.trim();

        let (utility_name, inner) = if let Some(inner) = trimmed
            .strip_prefix("key-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("key-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("value-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("value-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("properties-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("public-properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("public-properties-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("protected-properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("protected-properties-of", inner.trim())
        } else if let Some(inner) = trimmed
            .strip_prefix("private-properties-of<")
            .and_then(|s| s.strip_suffix('>'))
        {
            ("private-properties-of", inner.trim())
        } else {
            return None;
        };

        if inner.is_empty() {
            return Some(match utility_name {
                "key-of" => TUnion::array_key(),
                "value-of" => TUnion::mixed(),
                "properties-of"
                | "public-properties-of"
                | "protected-properties-of"
                | "private-properties-of" => TUnion::new(TAtomic::TArray {
                    key_type: Box::new(TUnion::string()),
                    value_type: Box::new(TUnion::mixed()),
                }),
                _ => unreachable!(),
            });
        }

        if utility_name == "key-of"
            && let Some(template_key_of) =
                self.try_resolve_template_key_of_type(trimmed, template_map)
        {
            return Some(template_key_of);
        }

        if utility_name == "value-of"
            && let Some(template_value_of) =
                self.try_resolve_template_value_of_type(trimmed, template_map)
        {
            return Some(template_value_of);
        }

        let parsed_inner = crate::docblock::parse_type_string(inner, self.interner);
        let resolved_inner =
            self.resolve_docblock_union_type(parsed_inner, self_class, parent_class, template_map);
        let expanded_inner = self.expand_docblock_class_constant_wildcards(
            resolved_inner,
            self_class,
            parent_class,
            class_constants,
        );

        Some(match utility_name {
            "key-of" => resolve_key_of_template_union(&expanded_inner),
            "value-of" => resolve_value_of_template_union(&expanded_inner),
            "properties-of" => self.resolve_properties_of_union(&expanded_inner, None),
            "public-properties-of" => {
                self.resolve_properties_of_union(&expanded_inner, Some(Visibility::Public))
            }
            "protected-properties-of" => {
                self.resolve_properties_of_union(&expanded_inner, Some(Visibility::Protected))
            }
            "private-properties-of" => {
                self.resolve_properties_of_union(&expanded_inner, Some(Visibility::Private))
            }
            _ => unreachable!(),
        })
    }

    fn resolve_properties_of_union(
        &self,
        union: &TUnion,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        let mut resolved_union = TUnion::nothing();

        for atomic in &union.types {
            let resolved_atomic = self.resolve_properties_of_atomic(atomic, visibility_filter);
            resolved_union = if resolved_union.is_nothing() {
                resolved_atomic
            } else {
                combine_union_types(&resolved_union, &resolved_atomic, false)
            };
        }

        if resolved_union.is_nothing() {
            TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::string()),
                value_type: Box::new(TUnion::mixed()),
            })
        } else {
            resolved_union
        }
    }

    fn resolve_properties_of_atomic(
        &self,
        atomic: &TAtomic,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        match atomic {
            TAtomic::TNamedObject { name, .. } => {
                self.resolve_properties_of_named_object(*name, visibility_filter)
            }
            TAtomic::TObjectIntersection { types } => {
                for intersection_atomic in types {
                    if let TAtomic::TNamedObject { name, .. } = intersection_atomic {
                        let resolved =
                            self.resolve_properties_of_named_object(*name, visibility_filter);
                        if !resolved.is_nothing() {
                            return resolved;
                        }
                    }
                }

                TUnion::nothing()
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                self.resolve_properties_of_union(as_type, visibility_filter)
            }
            _ => TUnion::nothing(),
        }
    }

    fn resolve_properties_of_named_object(
        &self,
        class_name: StrId,
        visibility_filter: Option<Visibility>,
    ) -> TUnion {
        let mut current_class_name = Some(class_name);
        let mut all_sealed = true;
        let mut properties = FxHashMap::default();

        while let Some(current_name) = current_class_name {
            let Some(class_info) = self
                .declarations
                .classes
                .iter()
                .find(|class_info| class_info.name == current_name)
            else {
                break;
            };

            if !class_info.is_final {
                all_sealed = false;
            }

            for property in class_info.properties.values() {
                let Some(property_type) = property.get_type() else {
                    continue;
                };

                if let Some(required_visibility) = visibility_filter
                    && property.visibility != required_visibility
                {
                    continue;
                }

                if property.is_static {
                    continue;
                }

                let property_name = self.interner.lookup(property.name).to_string();
                let property_key = pzoom_code_info::t_atomic::ArrayKey::String(property_name);

                if properties.contains_key(&property_key) {
                    continue;
                }

                properties.insert(property_key, property_type.clone());
            }

            current_class_name = class_info.parent_class;
        }

        if properties.is_empty() {
            return TUnion::nothing();
        }

        let (sealed, fallback_key_type, fallback_value_type) = if all_sealed {
            (true, None, None)
        } else {
            (
                false,
                Some(Box::new(TUnion::string())),
                Some(Box::new(TUnion::mixed())),
            )
        };

        TUnion::new(TAtomic::TKeyedArray {
            properties,
            is_list: false,
            sealed,
            fallback_key_type,
            fallback_value_type,
        })
    }

    fn get_docblock_return_type(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<(TUnion, Option<ConditionalReturnType>)> {
        let type_str = parsed
            .get_return()
            .and_then(crate::docblock::extract_type_string_from_content)?;

        if !self.is_valid_docblock_type_string(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            return None;
        }

        let mut resolved_type = if let Some(utility_type) = self.try_resolve_docblock_utility_type(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            utility_type
        } else {
            let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );

            self.expand_docblock_class_constant_wildcards(
                resolved_type,
                self_class,
                parent_class,
                class_constants,
            )
        };

        resolved_type.from_docblock = true;

        let conditional_return_type = self.parse_docblock_conditional_return_type(
            type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        Some((resolved_type, conditional_return_type))
    }

    fn get_docblock_if_this_is_type(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        for key in ["psalm-if-this-is", "phpstan-if-this-is", "if-this-is"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                let resolved_type = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );

                return Some(self.expand_docblock_class_constant_wildcards(
                    resolved_type,
                    self_class,
                    parent_class,
                    class_constants,
                ));
            }
        }

        None
    }

    fn parse_docblock_conditional_return_type(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<ConditionalReturnType> {
        let conditional_parts = crate::docblock::extract_conditional_type_parts(type_str)?;

        let if_true_type = self.parse_docblock_conditional_branch(
            &conditional_parts.if_true,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );
        let if_false_type = self.parse_docblock_conditional_branch(
            &conditional_parts.if_false,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        let condition = self.parse_docblock_conditional_condition(
            &conditional_parts.condition,
            self_class,
            parent_class,
            template_map,
            class_constants,
        )?;

        Some(ConditionalReturnType {
            condition,
            if_true_type,
            if_false_type,
        })
    }

    fn parse_docblock_conditional_branch(
        &mut self,
        branch_type: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> TUnion {
        if let Some(utility_type) = self.try_resolve_docblock_utility_type(
            branch_type,
            self_class,
            parent_class,
            template_map,
            class_constants,
        ) {
            return utility_type;
        }

        let parsed_branch_type = crate::docblock::parse_type_string(branch_type, self.interner);
        let resolved_branch_type = self.resolve_docblock_union_type(
            parsed_branch_type,
            self_class,
            parent_class,
            template_map,
        );

        self.expand_docblock_class_constant_wildcards(
            resolved_branch_type,
            self_class,
            parent_class,
            class_constants,
        )
    }

    fn parse_docblock_conditional_condition(
        &mut self,
        condition: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<ConditionalReturnCondition> {
        let normalized = condition.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized = normalized.trim();

        if let Some(rest) = normalized.strip_prefix("func_num_args() is ") {
            let count = rest.trim().parse::<usize>().ok()?;
            return Some(ConditionalReturnCondition::FuncNumArgsIs { count });
        }

        let (template_name, asserted_type_str) = normalized.split_once(" is ")?;
        let template_name = template_name.trim();
        if template_name.is_empty() {
            return None;
        }

        let template_name_id = template_map
            .and_then(|map| map.get(template_name))
            .map(|binding| binding.name)?;

        let asserted_type = self.parse_docblock_conditional_branch(
            asserted_type_str,
            self_class,
            parent_class,
            template_map,
            class_constants,
        );

        Some(ConditionalReturnCondition::TemplateIs {
            template_name: template_name_id,
            asserted_type,
        })
    }

    fn get_docblock_assertions(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> ParsedFunctionAssertions {
        let mut parsed_assertions = ParsedFunctionAssertions::default();

        for key in ["psalm-assert", "phpstan-assert", "assert"] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.assertions.push(assertion);
                    }
                }
            }
        }

        for key in [
            "psalm-assert-if-true",
            "phpstan-assert-if-true",
            "assert-if-true",
        ] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.if_true_assertions.push(assertion);
                    }
                }
            }
        }

        for key in [
            "psalm-assert-if-false",
            "phpstan-assert-if-false",
            "assert-if-false",
        ] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    if let Some(assertion) = self.parse_assertion_tag_content(
                        content,
                        self_class,
                        parent_class,
                        template_map,
                    ) {
                        parsed_assertions.if_false_assertions.push(assertion);
                    }
                }
            }
        }

        for key in ["psalm-this-out", "phpstan-this-out", "this-out"] {
            if let Some(tags) = parsed.tags.get(key) {
                for content in tags.values() {
                    let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                    else {
                        continue;
                    };

                    let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                    let parsed_type = self.resolve_docblock_union_type(
                        parsed_type,
                        self_class,
                        parent_class,
                        template_map,
                    );

                    parsed_assertions.assertions.push(Assertion {
                        var_id: self.interner.intern("$this"),
                        assertion_type: AssertionType::IsType(parsed_type),
                    });
                }
            }
        }

        parsed_assertions
    }

    fn parse_assertion_tag_content(
        &mut self,
        content: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> Option<Assertion> {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return None;
        }

        let type_str = crate::docblock::extract_type_string_from_content(trimmed)?;
        let remainder = trimmed.strip_prefix(type_str)?.trim_start();
        let var_token = remainder.split_whitespace().next()?;
        if !var_token.starts_with('$') {
            return None;
        }

        let mut assertion_source = type_str.trim();
        let mut is_negation = false;
        let mut is_loose_equality = false;
        let mut is_strict_equality = false;

        if let Some(rest) = assertion_source.strip_prefix('!') {
            is_negation = true;
            assertion_source = rest.trim_start();
        }

        if let Some(rest) = assertion_source.strip_prefix('~') {
            is_loose_equality = true;
            assertion_source = rest.trim_start();
        }

        if let Some(rest) = assertion_source.strip_prefix('=') {
            is_strict_equality = true;
            assertion_source = rest.trim_start();
        }

        if assertion_source.is_empty() {
            return None;
        }

        let assertion_type = if assertion_source.eq_ignore_ascii_case("truthy") {
            if is_negation {
                AssertionType::Falsy
            } else {
                AssertionType::Truthy
            }
        } else if assertion_source.eq_ignore_ascii_case("falsy")
            || assertion_source.eq_ignore_ascii_case("empty")
        {
            if is_negation {
                AssertionType::Truthy
            } else {
                AssertionType::Falsy
            }
        } else if assertion_source.eq_ignore_ascii_case("not-empty")
            || assertion_source.eq_ignore_ascii_case("non-empty")
        {
            if is_negation {
                AssertionType::Falsy
            } else {
                AssertionType::NotEmpty
            }
        } else if assertion_source.eq_ignore_ascii_case("not-null") {
            if is_negation {
                AssertionType::IsType(TUnion::new(TAtomic::TNull))
            } else {
                AssertionType::NotNull
            }
        } else {
            let parsed_type = crate::docblock::parse_type_string(assertion_source, self.interner);
            let parsed_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );

            if parsed_type.is_single()
                && matches!(parsed_type.get_single(), Some(TAtomic::TNull))
                && is_negation
                && !is_loose_equality
                && !is_strict_equality
            {
                AssertionType::NotNull
            } else if is_negation {
                if is_strict_equality {
                    AssertionType::IsNotEqual(parsed_type)
                } else if is_loose_equality {
                    AssertionType::IsNotLooselyEqual(parsed_type)
                } else {
                    AssertionType::IsNotType(parsed_type)
                }
            } else if is_strict_equality {
                AssertionType::IsEqual(parsed_type)
            } else if is_loose_equality {
                AssertionType::IsLooselyEqual(parsed_type)
            } else {
                AssertionType::IsType(parsed_type)
            }
        };

        Some(Assertion {
            var_id: self.interner.intern(var_token),
            assertion_type,
        })
    }

    fn get_implicit_assertions(
        &mut self,
        statements: &[Statement<'_>],
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        let mut assertions = Vec::new();

        for statement in statements {
            let Statement::If(if_stmt) = statement else {
                continue;
            };

            if if_stmt.body.has_else_clause() || if_stmt.body.has_else_if_clauses() {
                continue;
            }

            if !self.statements_throw(if_stmt.body.statements()) {
                continue;
            }

            assertions.extend(self.extract_assertions_when_false(
                if_stmt.condition,
                self_class,
                parent_class,
            ));
        }

        assertions
    }

    fn collect_defined_constants_from_statements(
        &mut self,
        statements: &[Statement<'_>],
    ) -> Vec<(StrId, TUnion)> {
        let mut defined_constants = Vec::new();

        for statement in statements {
            let Statement::Expression(expr_stmt) = statement else {
                continue;
            };
            let Expression::Call(mago_syntax::ast::ast::call::Call::Function(function_call)) =
                expr_stmt.expression.unparenthesized()
            else {
                continue;
            };
            let Expression::Identifier(function_name) = function_call.function.unparenthesized()
            else {
                continue;
            };
            if !function_name.value().eq_ignore_ascii_case("define") {
                continue;
            }

            let Some(name_arg) = function_call.argument_list.arguments.first() else {
                continue;
            };
            let Some(value_arg) = function_call.argument_list.arguments.get(1) else {
                continue;
            };

            let Expression::Literal(Literal::String(name_literal)) =
                name_arg.value().unparenthesized()
            else {
                continue;
            };
            let Some(constant_name) = name_literal.value else {
                continue;
            };

            let constant_name = constant_name.trim_start_matches('\\');
            if constant_name.is_empty() {
                continue;
            }

            let qualified_name = if constant_name.contains('\\') {
                constant_name.to_string()
            } else if let Some(namespace) = self.current_namespace {
                let namespace = self.interner.lookup(namespace);
                format!("{}\\{}", namespace, constant_name)
            } else {
                constant_name.to_string()
            };

            let constant_id = self.interner.intern(&qualified_name);
            let constant_type =
                infer_simple_expression_type(value_arg.value()).unwrap_or_else(TUnion::mixed);

            if let Some((_, existing_type)) = defined_constants
                .iter_mut()
                .find(|(existing_id, _)| *existing_id == constant_id)
            {
                *existing_type = constant_type;
            } else {
                defined_constants.push((constant_id, constant_type));
            }
        }

        defined_constants
    }

    fn span_contains_variadic_builtin_calls(&self, start_offset: u32, end_offset: u32) -> bool {
        let start = start_offset as usize;
        let end = end_offset as usize;

        if start >= end || end > self.source.len() {
            return false;
        }

        let haystack = self.source[start..end].to_ascii_lowercase();
        haystack.contains("func_get_arg(")
            || haystack.contains("func_get_args(")
            || haystack.contains("func_num_args(")
    }

    fn statements_throw(&self, statements: &[Statement<'_>]) -> bool {
        if statements.len() != 1 {
            return false;
        }

        match &statements[0] {
            Statement::Expression(expr_stmt) => {
                matches!(expr_stmt.expression.unparenthesized(), Expression::Throw(_))
            }
            Statement::Block(block) => self.statements_throw(block.statements.as_slice()),
            _ => false,
        }
    }

    fn extract_assertions_when_false(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        match expr.unparenthesized() {
            Expression::UnaryPrefix(unary) if unary.operator.is_not() => {
                self.extract_assertions_when_true(unary.operand, self_class, parent_class)
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Or(_) | BinaryOperator::LowOr(_)
                ) =>
            {
                let mut assertions =
                    self.extract_assertions_when_false(binary.lhs, self_class, parent_class);
                assertions.extend(self.extract_assertions_when_false(
                    binary.rhs,
                    self_class,
                    parent_class,
                ));
                assertions
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
                ) =>
            {
                if let Some(var_name) = extract_direct_var(binary.lhs) {
                    if is_null_expression(binary.rhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }
                }

                if let Some(var_name) = extract_direct_var(binary.rhs) {
                    if is_null_expression(binary.lhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }
                }

                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn extract_assertions_when_true(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        match expr.unparenthesized() {
            Expression::Binary(binary) if binary.operator.is_instanceof() => {
                let Some(var_name) = extract_direct_var(binary.lhs) else {
                    return Vec::new();
                };
                let Some(class_id) =
                    self.resolve_class_expression(binary.rhs, self_class, parent_class)
                else {
                    return Vec::new();
                };

                vec![Assertion {
                    var_id: self.interner.intern(&var_name),
                    assertion_type: AssertionType::IsType(TUnion::new(TAtomic::TNamedObject {
                        name: class_id,
                        type_params: None,
                    })),
                }]
            }
            Expression::Binary(binary)
                if matches!(
                    binary.operator,
                    BinaryOperator::NotEqual(_)
                        | BinaryOperator::AngledNotEqual(_)
                        | BinaryOperator::NotIdentical(_)
                ) =>
            {
                if let Some(var_name) = extract_direct_var(binary.lhs) {
                    if is_null_expression(binary.rhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }
                }

                if let Some(var_name) = extract_direct_var(binary.rhs) {
                    if is_null_expression(binary.lhs) {
                        return vec![Assertion {
                            var_id: self.interner.intern(&var_name),
                            assertion_type: AssertionType::NotNull,
                        }];
                    }
                }

                Vec::new()
            }
            Expression::Call(call) => {
                self.extract_builtin_call_assertions(call, self_class, parent_class)
            }
            _ => Vec::new(),
        }
    }

    fn extract_builtin_call_assertions(
        &mut self,
        call: &mago_syntax::ast::ast::call::Call<'_>,
        _self_class: Option<StrId>,
        _parent_class: Option<StrId>,
    ) -> Vec<Assertion> {
        let mago_syntax::ast::ast::call::Call::Function(function_call) = call else {
            return Vec::new();
        };

        let Expression::Identifier(function_name) = function_call.function.unparenthesized() else {
            return Vec::new();
        };

        let Some(first_arg) = function_call.argument_list.arguments.first() else {
            return Vec::new();
        };
        let Some(var_name) = extract_direct_var(first_arg.value()) else {
            return Vec::new();
        };

        let asserted_type = match function_name.value().to_ascii_lowercase().as_str() {
            "is_string" => TAtomic::TString,
            "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
            "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
            "is_bool" => TAtomic::TBool,
            "is_object" => TAtomic::TObject,
            "is_null" => TAtomic::TNull,
            "is_numeric" => TAtomic::TNumeric,
            "is_resource" => TAtomic::TResource,
            "is_scalar" => TAtomic::TScalar,
            "is_array" => TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            _ => return Vec::new(),
        };

        vec![Assertion {
            var_id: self.interner.intern(&var_name),
            assertion_type: AssertionType::IsType(TUnion::new(asserted_type)),
        }]
    }

    fn resolve_class_expression(
        &mut self,
        expr: &Expression<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> Option<StrId> {
        match expr.unparenthesized() {
            Expression::Identifier(identifier) => Some(self.resolve_identifier(identifier)),
            Expression::Self_(_) | Expression::Static(_) => self_class.or(Some(StrId::SELF)),
            Expression::Parent(_) => parent_class.or(Some(StrId::PARENT)),
            _ => None,
        }
    }

    fn register_namespace_type_aliases(
        &mut self,
        aliases: &FxHashMap<String, TUnion>,
        start_offset: u32,
    ) {
        for (alias_name, aliased_type) in aliases {
            let scoped_alias = self.make_fqn(alias_name);
            self.declarations.type_aliases.push(TypeAliasInfo {
                name: scoped_alias,
                aliased_type: aliased_type.clone(),
                file_path: self.file_path,
                start_offset,
            });
        }
    }

    fn collect_preceding_statement_type_aliases(&mut self, stmt_start_offset: u32) {
        let docblocks: Vec<&'p str> = self
            .trivia
            .iter()
            .filter(|trivia| {
                trivia.kind == TriviaKind::DocBlockComment
                    && trivia.span.end.offset < stmt_start_offset
            })
            .map(|trivia| trivia.value)
            .collect();

        for docblock in docblocks {
            let parsed = crate::docblock::parse(docblock, 0);
            if !(parsed.tags.contains_key("phpstan-type")
                || parsed.tags.contains_key("psalm-type")
                || parsed.tags.contains_key("phpstan-import-type")
                || parsed.tags.contains_key("psalm-import-type"))
            {
                continue;
            }

            let aliases = self.collect_docblock_type_aliases(&parsed, None, None, None, None);
            self.register_namespace_type_aliases(&aliases, stmt_start_offset);
        }
    }

    fn collect_docblock_type_aliases(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        base_aliases: Option<&FxHashMap<String, TUnion>>,
    ) -> FxHashMap<String, TUnion> {
        let mut aliases = base_aliases.cloned().unwrap_or_default();

        let mut import_entries: Vec<(usize, String)> = Vec::new();
        for key in ["phpstan-import-type", "psalm-import-type"] {
            if let Some(tags) = parsed.tags.get(key) {
                for (offset, content) in tags {
                    import_entries.push((*offset, content.clone()));
                }
            }
        }
        import_entries.sort_by_key(|(offset, _)| *offset);

        for (_, content) in import_entries {
            let Some((imported_alias, source_name, alias_name)) =
                parse_import_type_tag_content(&content)
            else {
                continue;
            };

            let source_class = self.resolve_docblock_class_name(
                self.interner.intern(&source_name),
                self_class,
                parent_class,
            );
            let scoped_alias = self.interner.intern(&format!(
                "{}::{}",
                self.interner.lookup(source_class),
                imported_alias
            ));

            if let Some(type_alias) = self.known_type_aliases.get(&scoped_alias) {
                aliases.insert(alias_name, type_alias.aliased_type.clone());
                continue;
            }

            if let Some(type_alias) = self
                .declarations
                .type_aliases
                .iter()
                .find(|type_alias| type_alias.name == scoped_alias)
            {
                aliases.insert(alias_name, type_alias.aliased_type.clone());
                continue;
            }

            // Keep unresolved imported aliases from triggering UndefinedClass.
            aliases.insert(alias_name, TUnion::mixed());
        }

        let mut type_entries: Vec<(usize, String)> = Vec::new();
        for key in ["phpstan-type", "psalm-type"] {
            if let Some(tags) = parsed.tags.get(key) {
                for (offset, content) in tags {
                    type_entries.push((*offset, content.clone()));
                }
            }
        }
        type_entries.sort_by_key(|(offset, _)| *offset);

        for (_, content) in type_entries {
            let Some((alias_name, type_definition)) = parse_type_alias_tag_content(&content) else {
                continue;
            };

            let previous_aliases =
                std::mem::replace(&mut self.active_docblock_type_aliases, aliases.clone());
            let parsed_type = crate::docblock::parse_type_string(&type_definition, self.interner);
            let resolved_type = self.resolve_docblock_union_type(
                parsed_type,
                self_class,
                parent_class,
                template_map,
            );
            self.active_docblock_type_aliases = previous_aliases;

            aliases.insert(alias_name, resolved_type);
        }

        aliases
    }

    fn resolve_docblock_union_type(
        &mut self,
        mut t_union: TUnion,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) -> TUnion {
        let mut resolved_types = Vec::new();
        for mut atomic in t_union.types {
            if let Some(alias_union) = self.resolve_docblock_type_alias_atomic(&atomic) {
                for alias_atomic in alias_union.types {
                    if !resolved_types.contains(&alias_atomic) {
                        resolved_types.push(alias_atomic);
                    }
                }
                continue;
            }

            self.resolve_docblock_atomic_type(&mut atomic, self_class, parent_class, template_map);
            if !resolved_types.contains(&atomic) {
                resolved_types.push(atomic);
            }
        }

        t_union.types = resolved_types;
        t_union.is_nullable = t_union.types.iter().any(TAtomic::is_nullable);
        t_union.is_falsable = t_union.types.iter().any(TAtomic::is_falsable);
        t_union
    }

    fn resolve_docblock_type_alias_atomic(&self, atomic: &TAtomic) -> Option<TUnion> {
        let TAtomic::TNamedObject { name, type_params } = atomic else {
            return None;
        };

        if type_params.is_some() {
            return None;
        }

        let alias_name = self.interner.lookup(*name);
        if let Some(alias_union) = self.active_docblock_type_aliases.get(alias_name.as_ref()) {
            return Some(alias_union.clone());
        }

        let fqn_alias = if let Some(ns) = self.current_namespace {
            let ns_str = self.interner.lookup(ns);
            self.interner
                .intern(&format!("{}\\{}", ns_str, alias_name.as_ref()))
        } else {
            self.interner.intern(alias_name.as_ref())
        };
        if let Some(type_alias) = self.known_type_aliases.get(&fqn_alias) {
            return Some(type_alias.aliased_type.clone());
        }

        if let Some(type_alias) = self
            .declarations
            .type_aliases
            .iter()
            .rev()
            .find(|type_alias| type_alias.name == fqn_alias)
        {
            return Some(type_alias.aliased_type.clone());
        }

        let raw_alias = self.interner.intern(alias_name.as_ref());
        if let Some(type_alias) = self.known_type_aliases.get(&raw_alias) {
            return Some(type_alias.aliased_type.clone());
        }

        self.declarations
            .type_aliases
            .iter()
            .rev()
            .find(|type_alias| type_alias.name == raw_alias)
            .map(|type_alias| type_alias.aliased_type.clone())
    }

    fn resolve_docblock_atomic_type(
        &mut self,
        atomic: &mut TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
    ) {
        match atomic {
            TAtomic::TNamedObject { name, type_params } => {
                if type_params.is_none() {
                    let template_key = self.interner.lookup(*name);
                    if let Some(template_binding) =
                        template_map.and_then(|map| map.get(template_key.as_ref()))
                    {
                        *atomic = TAtomic::TTemplateParam {
                            name: template_binding.name,
                            defining_entity: template_binding.defining_entity,
                            as_type: Box::new(template_binding.as_type.clone()),
                        };
                        return;
                    }
                }

                *name = self.resolve_docblock_class_name(*name, self_class, parent_class);
                if let Some(type_params) = type_params {
                    for param in type_params {
                        *param = self.resolve_docblock_union_type(
                            param.clone(),
                            self_class,
                            parent_class,
                            template_map,
                        );
                    }
                }
            }
            TAtomic::TArray {
                key_type,
                value_type,
            }
            | TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            }
            | TAtomic::TIterable {
                key_type,
                value_type,
            } => {
                **key_type = self.resolve_docblock_union_type(
                    (**key_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
                **value_type = self.resolve_docblock_union_type(
                    (**value_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                **value_type = self.resolve_docblock_union_type(
                    (**value_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                for prop_type in properties.values_mut() {
                    *prop_type = self.resolve_docblock_union_type(
                        prop_type.clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
                if let Some(key_type) = fallback_key_type {
                    **key_type = self.resolve_docblock_union_type(
                        (**key_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
                if let Some(value_type) = fallback_value_type {
                    **value_type = self.resolve_docblock_union_type(
                        (**value_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                **as_type = self.resolve_docblock_union_type(
                    (**as_type).clone(),
                    self_class,
                    parent_class,
                    template_map,
                );
            }
            TAtomic::TTemplateParamClass { as_type, .. } => {
                self.resolve_docblock_atomic_type(as_type, self_class, parent_class, template_map);
            }
            TAtomic::TClosure {
                params,
                return_type,
                ..
            }
            | TAtomic::TCallable {
                params,
                return_type,
                ..
            } => {
                if let Some(params) = params {
                    for param in params {
                        param.param_type = self.resolve_docblock_union_type(
                            param.param_type.clone(),
                            self_class,
                            parent_class,
                            template_map,
                        );
                    }
                }

                if let Some(return_type) = return_type {
                    **return_type = self.resolve_docblock_union_type(
                        (**return_type).clone(),
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TClassString { as_type } => {
                if let Some(as_type) = as_type {
                    self.resolve_docblock_atomic_type(
                        as_type,
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for atomic in types.iter_mut() {
                    self.resolve_docblock_atomic_type(
                        atomic,
                        self_class,
                        parent_class,
                        template_map,
                    );
                }
            }
            _ => {}
        }
    }

    fn resolve_docblock_class_name(
        &mut self,
        name: StrId,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> StrId {
        let name_str = self.interner.lookup(name);
        let normalized = name_str.as_ref().trim();

        if let Some((class_part, const_part)) = normalized.rsplit_once("::") {
            let resolved_class = self.resolve_docblock_class_name(
                self.interner.intern(class_part),
                self_class,
                parent_class,
            );
            let resolved_class_name = self.interner.lookup(resolved_class);
            return self
                .interner
                .intern(&format!("{}::{}", resolved_class_name, const_part));
        }

        let lower = normalized.to_ascii_lowercase();

        if lower == "self" {
            return self_class.unwrap_or(StrId::SELF);
        }

        if lower == "static" {
            return StrId::STATIC;
        }

        if lower == "parent" {
            return parent_class.unwrap_or(StrId::PARENT);
        }

        if normalized.starts_with('\\') {
            return self
                .interner
                .intern(normalized.strip_prefix('\\').unwrap_or(normalized));
        }

        let (first_segment, remainder) = match normalized.split_once('\\') {
            Some((first, rest)) => (first, Some(rest)),
            None => (normalized, None),
        };

        if let Some(alias_target) = self.use_aliases.get(&first_segment.to_ascii_lowercase()) {
            if let Some(remainder) = remainder {
                let alias_str = self.interner.lookup(*alias_target);
                return self
                    .interner
                    .intern(&format!("{}\\{}", alias_str, remainder));
            }

            return *alias_target;
        }

        if let Some(current_namespace) = self.current_namespace {
            let namespace = self.interner.lookup(current_namespace);
            return self
                .interner
                .intern(&format!("{}\\{}", namespace, normalized));
        }

        self.interner.intern(normalized)
    }

    fn expand_docblock_class_constant_wildcards(
        &mut self,
        t_union: TUnion,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> TUnion {
        let mut expanded_types = Vec::new();

        for atomic in t_union.types {
            for expanded_atomic in self.expand_docblock_class_constant_wildcards_in_atomic(
                atomic,
                self_class,
                parent_class,
                class_constants,
            ) {
                if !expanded_types.contains(&expanded_atomic) {
                    expanded_types.push(expanded_atomic);
                }
            }
        }

        TUnion::from_types(expanded_types)
    }

    fn expand_docblock_class_constant_wildcards_in_atomic(
        &mut self,
        atomic: TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Vec<TAtomic> {
        if let Some(expanded_union) = self.resolve_class_constant_union_from_atomic(
            &atomic,
            self_class,
            parent_class,
            class_constants,
        ) {
            return expanded_union.types;
        }

        match atomic {
            TAtomic::TArray {
                key_type,
                value_type,
            } => vec![TAtomic::TArray {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            } => vec![TAtomic::TNonEmptyArray {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TIterable {
                key_type,
                value_type,
            } => vec![TAtomic::TIterable {
                key_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *key_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TList { value_type } => vec![TAtomic::TList {
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TNonEmptyList { value_type } => vec![TAtomic::TNonEmptyList {
                value_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *value_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            TAtomic::TKeyedArray {
                properties,
                is_list,
                sealed,
                fallback_key_type,
                fallback_value_type,
            } => vec![TAtomic::TKeyedArray {
                properties: properties
                    .into_iter()
                    .map(|(key, prop_type)| {
                        (
                            key,
                            self.expand_docblock_class_constant_wildcards(
                                prop_type,
                                self_class,
                                parent_class,
                                class_constants,
                            ),
                        )
                    })
                    .collect(),
                is_list,
                sealed,
                fallback_key_type: fallback_key_type.map(|key_type| {
                    Box::new(self.expand_docblock_class_constant_wildcards(
                        *key_type,
                        self_class,
                        parent_class,
                        class_constants,
                    ))
                }),
                fallback_value_type: fallback_value_type.map(|value_type| {
                    Box::new(self.expand_docblock_class_constant_wildcards(
                        *value_type,
                        self_class,
                        parent_class,
                        class_constants,
                    ))
                }),
            }],
            TAtomic::TNamedObject { name, type_params } => {
                if let Some(type_params) = type_params {
                    vec![TAtomic::TNamedObject {
                        name,
                        type_params: Some(
                            type_params
                                .into_iter()
                                .map(|type_param| {
                                    self.expand_docblock_class_constant_wildcards(
                                        type_param,
                                        self_class,
                                        parent_class,
                                        class_constants,
                                    )
                                })
                                .collect(),
                        ),
                    }]
                } else {
                    vec![TAtomic::TNamedObject {
                        name,
                        type_params: None,
                    }]
                }
            }
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => vec![TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type: Box::new(self.expand_docblock_class_constant_wildcards(
                    *as_type,
                    self_class,
                    parent_class,
                    class_constants,
                )),
            }],
            other => vec![other],
        }
    }

    fn resolve_class_constant_union_from_atomic(
        &mut self,
        atomic: &TAtomic,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Option<TUnion> {
        let TAtomic::TNamedObject { name, type_params } = atomic else {
            return None;
        };

        if type_params.is_some() {
            return None;
        }

        let raw_name = self.interner.lookup(*name).to_string();
        let (class_part, constant_part) = raw_name.split_once("::")?;
        if constant_part.eq_ignore_ascii_case("class") {
            return None;
        }

        let class_part = class_part.trim();
        let constant_part = constant_part.trim();
        if class_part.is_empty() || constant_part.is_empty() {
            return None;
        }

        let class_part_lower = class_part.to_ascii_lowercase();
        let resolved_class = match class_part_lower.as_str() {
            "self" | "static" => self_class?,
            "parent" => parent_class?,
            _ => {
                let class_name = self.interner.intern(class_part);
                self.resolve_docblock_class_name(class_name, self_class, parent_class)
            }
        };

        let constants = if Some(resolved_class) == self_class {
            class_constants
        } else {
            self.declarations
                .classes
                .iter()
                .find(|class_info| class_info.name == resolved_class)
                .map(|class_info| &class_info.constants)
        };

        let Some(constants) = constants else {
            return Some(TUnion::array_key());
        };

        let mut resolved_union: Option<TUnion> = None;

        if let Some(prefix) = constant_part.strip_suffix('*') {
            for (constant_name, constant_info) in constants {
                let candidate_name = self.interner.lookup(*constant_name);
                if candidate_name.starts_with(prefix) {
                    resolved_union = Some(if let Some(existing) = resolved_union {
                        combine_union_types(&existing, &constant_info.constant_type, false)
                    } else {
                        constant_info.constant_type.clone()
                    });
                }
            }
        } else {
            for (constant_name, constant_info) in constants {
                if self.interner.lookup(*constant_name).as_ref() == constant_part {
                    resolved_union = Some(constant_info.constant_type.clone());
                    break;
                }
            }
        }

        if resolved_union.is_none() {
            // Keep docblock parsing permissive when constants cannot be resolved
            // during scanning (e.g. ordering/population gaps).
            return Some(TUnion::array_key());
        }

        resolved_union
    }

    fn parse_docblock_template_bindings(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        defining_entity: StrId,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        base_template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> Vec<DocblockTemplateBinding> {
        let mut template_entries = Vec::new();

        if let Some(tags) = parsed.combined_tags.get("template") {
            for (offset, content) in tags {
                template_entries.push((*offset, content.as_str(), TemplateVariance::Invariant));
            }
        }

        if let Some(tags) = parsed.combined_tags.get("template-covariant") {
            for (offset, content) in tags {
                template_entries.push((*offset, content.as_str(), TemplateVariance::Covariant));
            }
        }

        if template_entries.is_empty() {
            return Vec::new();
        }

        template_entries.sort_by_key(|(offset, _, _)| *offset);

        let mut template_map = base_template_map.cloned().unwrap_or_default();
        let mut template_bindings: Vec<DocblockTemplateBinding> = Vec::new();

        for (_, content, variance) in template_entries {
            let Some((template_name, template_bound)) = parse_template_tag_content(content) else {
                continue;
            };

            let template_name_id = self.interner.intern(&template_name);
            let placeholder = DocblockTemplateBinding {
                name: template_name_id,
                defining_entity,
                as_type: TUnion::mixed(),
                variance,
            };
            template_map.insert(template_name.clone(), placeholder.clone());

            let as_type = if let Some(template_bound) = template_bound {
                self.try_resolve_template_key_of_type(&template_bound, Some(&template_map))
                    .unwrap_or_else(|| {
                        let parsed_type =
                            crate::docblock::parse_type_string(&template_bound, self.interner);
                        self.resolve_docblock_union_type(
                            parsed_type,
                            self_class,
                            parent_class,
                            Some(&template_map),
                        )
                    })
            } else {
                TUnion::mixed()
            };

            let as_type = self.expand_docblock_class_constant_wildcards(
                as_type,
                self_class,
                parent_class,
                class_constants,
            );

            let binding = DocblockTemplateBinding {
                as_type,
                ..placeholder
            };
            template_map.insert(template_name.clone(), binding.clone());

            if let Some(existing_binding) = template_bindings
                .iter_mut()
                .find(|existing| existing.name == template_name_id)
            {
                *existing_binding = binding;
            } else {
                template_bindings.push(binding);
            }
        }

        template_bindings
    }

    fn build_template_map_from_bindings(
        &self,
        bindings: &[DocblockTemplateBinding],
        base_template_map: Option<&TemplateMap>,
    ) -> TemplateMap {
        let mut template_map = base_template_map.cloned().unwrap_or_default();

        for binding in bindings {
            template_map.insert(
                self.interner.lookup(binding.name).to_string(),
                binding.clone(),
            );
        }

        template_map
    }

    fn build_template_map_from_class_template_types(
        &self,
        template_types: &[TemplateType],
        defining_entity: StrId,
    ) -> TemplateMap {
        let mut template_map = FxHashMap::default();

        for template_type in template_types {
            template_map.insert(
                self.interner.lookup(template_type.name).to_string(),
                DocblockTemplateBinding {
                    name: template_type.name,
                    defining_entity,
                    as_type: template_type.as_type.clone(),
                    variance: template_type.variance,
                },
            );
        }

        template_map
    }

    fn is_docblock_pure(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("pure")
            || parsed.tags.contains_key("psalm-pure")
            || parsed.tags.contains_key("phpstan-pure")
    }

    fn is_docblock_inheritdoc(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        if parsed
            .tags
            .keys()
            .any(|tag_name| tag_name.eq_ignore_ascii_case("inheritdoc"))
        {
            return true;
        }

        parsed
            .description
            .to_ascii_lowercase()
            .contains("@inheritdoc")
    }

    fn is_docblock_mutation_free(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("mutation-free")
            || parsed.tags.contains_key("psalm-mutation-free")
            || parsed.tags.contains_key("phpstan-mutation-free")
    }

    fn is_docblock_no_named_arguments(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("no-named-arguments")
            || parsed.tags.contains_key("psalm-no-named-arguments")
            || parsed.tags.contains_key("phpstan-no-named-arguments")
    }

    fn is_docblock_ignore_nullable_return(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("ignore-nullable-return")
            || parsed.tags.contains_key("psalm-ignore-nullable-return")
            || parsed.tags.contains_key("phpstan-ignore-nullable-return")
    }

    fn is_docblock_ignore_falsable_return(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("ignore-falsable-return")
            || parsed.tags.contains_key("psalm-ignore-falsable-return")
            || parsed.tags.contains_key("phpstan-ignore-falsable-return")
    }

    fn is_docblock_immutable(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("immutable") || parsed.tags.contains_key("psalm-immutable")
    }

    fn is_docblock_final(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("final") || parsed.tags.contains_key("psalm-final")
    }

    fn is_docblock_consistent_constructor(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("consistent-constructor")
            || parsed.tags.contains_key("psalm-consistent-constructor")
    }

    fn is_docblock_no_seal_properties(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("no-seal-properties")
            || parsed.tags.contains_key("psalm-no-seal-properties")
    }

    fn get_docblock_sealed_properties(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<bool> {
        if parsed.tags.contains_key("seal-properties")
            || parsed.tags.contains_key("psalm-seal-properties")
        {
            return Some(true);
        }

        if parsed.tags.contains_key("no-seal-properties")
            || parsed.tags.contains_key("psalm-no-seal-properties")
        {
            return Some(false);
        }

        None
    }

    fn get_docblock_sealed_methods(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<bool> {
        if parsed.tags.contains_key("seal-methods")
            || parsed.tags.contains_key("psalm-seal-methods")
        {
            return Some(true);
        }

        if parsed.tags.contains_key("no-seal-methods")
            || parsed.tags.contains_key("psalm-no-seal-methods")
        {
            return Some(false);
        }

        None
    }

    fn is_docblock_deprecated(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("deprecated")
    }

    fn get_docblock_deprecation_message(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> Option<String> {
        let deprecated_tags = parsed.tags.get("deprecated")?;
        let mut ordered_tags: Vec<_> = deprecated_tags.iter().collect();
        ordered_tags.sort_by_key(|(offset, _)| *offset);

        ordered_tags.into_iter().find_map(|(_, content)| {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    fn get_docblock_internal_scopes(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
        defining_symbol: StrId,
        docblock_issues: &mut Vec<DocblockIssue>,
    ) -> Vec<StrId> {
        let mut scopes = Vec::new();

        if let Some(psalm_internal_tags) = parsed.tags.get("psalm-internal") {
            let mut ordered_tags: Vec<_> = psalm_internal_tags.iter().collect();
            ordered_tags.sort_by_key(|(offset, _)| *offset);

            for (offset, content) in ordered_tags {
                let normalized = content.trim().trim_start_matches('\\').trim();
                if normalized.is_empty() {
                    docblock_issues.push(DocblockIssue {
                        message: "psalm-internal annotation used without specifying namespace"
                            .to_string(),
                        start_offset: (*offset) as u32,
                        end_offset: (*offset) as u32 + 1,
                    });
                    continue;
                }

                let scope_id = self.interner.intern(normalized);
                if !scopes.contains(&scope_id) {
                    scopes.push(scope_id);
                }
            }

            if !scopes.is_empty() {
                return scopes;
            }
        }

        if parsed.tags.contains_key("internal") {
            let default_scope = self.get_default_internal_scope(defining_symbol);
            if !scopes.contains(&default_scope) {
                scopes.push(default_scope);
            }
        }

        scopes
    }

    fn get_default_internal_scope(&self, defining_symbol: StrId) -> StrId {
        let symbol = self.interner.lookup(defining_symbol);
        let normalized = symbol.trim_start_matches('\\');
        let namespace = normalized.rsplit_once('\\').map(|(ns, _)| ns).unwrap_or("");
        let top_level_namespace = namespace.split('\\').next().unwrap_or("");

        if top_level_namespace.is_empty() {
            StrId::EMPTY
        } else {
            self.interner.intern(top_level_namespace)
        }
    }

    fn has_attribute_named(
        &mut self,
        attribute_lists: &Sequence<'_, AttributeList<'_>>,
        expected_name: &str,
    ) -> bool {
        attribute_lists.iter().any(|attribute_list| {
            attribute_list.attributes.iter().any(|attribute| {
                let attribute_name = self.resolve_identifier(&attribute.name);
                let attribute_name = self.interner.lookup(attribute_name);
                let short_name = attribute_name
                    .as_ref()
                    .rsplit('\\')
                    .next()
                    .unwrap_or(attribute_name.as_ref());

                short_name.eq_ignore_ascii_case(expected_name)
            })
        })
    }

    fn get_attribute_flags(
        &mut self,
        class_like_name: StrId,
        attribute_lists: &Sequence<'_, AttributeList<'_>>,
    ) -> Option<u8> {
        let class_like_name = self.interner.lookup(class_like_name);
        let class_like_short_name = class_like_name
            .as_ref()
            .rsplit('\\')
            .next()
            .unwrap_or(class_like_name.as_ref());

        // Attribute itself can always be used on classes.
        if class_like_short_name.eq_ignore_ascii_case("Attribute") {
            return Some(1);
        }

        for attribute in attribute_lists
            .iter()
            .flat_map(|attribute_list| attribute_list.attributes.iter())
        {
            let attribute_name = self.resolve_identifier(&attribute.name);
            let attribute_name = self.interner.lookup(attribute_name);
            let attribute_short_name = attribute_name
                .as_ref()
                .rsplit('\\')
                .next()
                .unwrap_or(attribute_name.as_ref());

            if !attribute_short_name.eq_ignore_ascii_case("Attribute") {
                continue;
            }

            let Some(first_argument) = attribute
                .argument_list
                .as_ref()
                .and_then(|argument_list| argument_list.arguments.first())
            else {
                // No target specified means all targets.
                return Some(63);
            };

            let bits = self
                .eval_attribute_flag_expression(first_argument.value())
                .and_then(|v| u8::try_from(v).ok())
                .unwrap_or(127);

            return Some(bits);
        }

        None
    }

    fn eval_attribute_flag_expression(&mut self, expr: &Expression<'_>) -> Option<i64> {
        match expr.unparenthesized() {
            Expression::Literal(Literal::Integer(integer)) => {
                integer.value.and_then(|v| i64::try_from(v).ok())
            }
            Expression::Binary(binary) => {
                let left = self.eval_attribute_flag_expression(binary.lhs)?;
                let right = self.eval_attribute_flag_expression(binary.rhs)?;

                match binary.operator {
                    BinaryOperator::BitwiseOr(_) => Some(left | right),
                    BinaryOperator::BitwiseAnd(_) => Some(left & right),
                    BinaryOperator::BitwiseXor(_) => Some(left ^ right),
                    _ => None,
                }
            }
            Expression::Access(Access::ClassConstant(class_constant_access)) => {
                let class_name = match class_constant_access.class.unparenthesized() {
                    Expression::Identifier(identifier) => {
                        let resolved = self.resolve_identifier(identifier);
                        self.interner.lookup(resolved).to_string()
                    }
                    _ => return None,
                };

                let class_short_name = class_name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(class_name.as_str());
                if !class_short_name.eq_ignore_ascii_case("Attribute") {
                    return None;
                }

                let ClassLikeConstantSelector::Identifier(constant_name) =
                    &class_constant_access.constant
                else {
                    return None;
                };

                match constant_name.value.to_ascii_uppercase().as_str() {
                    "TARGET_CLASS" => Some(1),
                    "TARGET_FUNCTION" => Some(2),
                    "TARGET_METHOD" => Some(4),
                    "TARGET_PROPERTY" => Some(8),
                    "TARGET_CLASS_CONSTANT" => Some(16),
                    "TARGET_PARAMETER" => Some(32),
                    "TARGET_ALL" => Some(63),
                    "IS_REPEATABLE" => Some(64),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn push_docblock_issue(
        &self,
        class_info: &mut ClassLikeInfo,
        message: String,
        start_offset: u32,
        end_offset: u32,
    ) {
        class_info.docblock_issues.push(DocblockIssue {
            message,
            start_offset,
            end_offset,
        });
    }

    fn validate_property_docblock_tags(
        &self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        property_start: u32,
    ) {
        if parsed.tags.contains_key("property")
            || parsed.tags.contains_key("psalm-property")
            || parsed.tags.contains_key("phpstan-property")
            || parsed.tags.contains_key("property-read")
            || parsed.tags.contains_key("psalm-property-read")
            || parsed.tags.contains_key("phpstan-property-read")
            || parsed.tags.contains_key("property-write")
            || parsed.tags.contains_key("psalm-property-write")
            || parsed.tags.contains_key("phpstan-property-write")
            || parsed.tags.contains_key("method")
            || parsed.tags.contains_key("psalm-method")
            || parsed.tags.contains_key("mixin")
            || parsed.tags.contains_key("psalm-mixin")
            || parsed.tags.contains_key("phpstan-mixin")
        {
            self.push_docblock_issue(
                class_info,
                "Invalid docblock annotation on property".to_string(),
                property_start,
                property_start.saturating_add(1),
            );
        }
    }

    fn validate_type_alias_docblock_tags(
        &self,
        class_info: &mut ClassLikeInfo,
        parsed: &crate::docblock::ParsedDocblock,
        start_offset: u32,
    ) {
        for key in ["phpstan-type", "psalm-type"] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some((_, type_definition)) = parse_type_alias_tag_content(content) else {
                    self.push_docblock_issue(
                        class_info,
                        "Invalid type alias in docblock".to_string(),
                        start_offset,
                        start_offset.saturating_add(1),
                    );
                    continue;
                };

                if !has_balanced_type_delimiters(&type_definition) {
                    self.push_docblock_issue(
                        class_info,
                        "Invalid type alias in docblock".to_string(),
                        start_offset,
                        start_offset.saturating_add(1),
                    );
                }
            }
        }
    }

    fn validate_function_docblock_type_tags(
        &mut self,
        parsed: &crate::docblock::ParsedDocblock,
        start_offset: u32,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
        issues: &mut Vec<DocblockIssue>,
    ) {
        let mut typed_param_tags = FxHashSet::default();

        if parsed.tags.contains_key("var")
            || parsed.tags.contains_key("psalm-var")
            || parsed.tags.contains_key("phpstan-var")
            || parsed.tags.contains_key("import-type")
            || parsed.tags.contains_key("psalm-import-type")
            || parsed.tags.contains_key("phpstan-import-type")
        {
            issues.push(DocblockIssue {
                message: "Possibly invalid docblock tag".to_string(),
                start_offset,
                end_offset: start_offset.saturating_add(1),
            });
        }

        for key in [
            "param",
            "psalm-param",
            "phpstan-param",
            "param-out",
            "psalm-param-out",
            "phpstan-param-out",
        ] {
            let Some(tags) = parsed.tags.get(key) else {
                continue;
            };

            let mut seen_vars = FxHashSet::default();
            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    continue;
                }

                let Some(var_name) = crate::docblock::extract_var_name_from_content(content) else {
                    continue;
                };

                let normalized = var_name.trim_start_matches('$').to_string();
                if !seen_vars.insert(normalized) {
                    issues.push(DocblockIssue {
                        message: "Invalid docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    break;
                }
            }
        }

        for key in ["return", "psalm-return", "phpstan-return"] {
            if parsed.tags.get(key).is_some_and(|tags| tags.len() > 1) {
                issues.push(DocblockIssue {
                    message: "Invalid docblock type".to_string(),
                    start_offset,
                    end_offset: start_offset.saturating_add(1),
                });
            }
        }

        for key in ["param", "param-out"] {
            let Some(tags) = parsed.combined_tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    continue;
                }

                if let Some(var_name) = crate::docblock::extract_var_name_from_content(content) {
                    typed_param_tags.insert(var_name.trim_start_matches('$').to_string());
                }
            }
        }

        for key in ["param", "return", "param-out"] {
            let Some(tags) = parsed.combined_tags.get(key) else {
                continue;
            };

            for content in tags.values() {
                let Some(type_str) = crate::docblock::extract_type_string_from_content(content)
                else {
                    if key == "return" {
                        if content.trim().eq_ignore_ascii_case("$this") {
                            continue;
                        }

                        issues.push(DocblockIssue {
                            message: "Missing return docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                    }

                    if key == "param" || key == "param-out" {
                        if let Some(var_name) =
                            crate::docblock::extract_var_name_from_content(content)
                        {
                            if typed_param_tags.contains(var_name.trim_start_matches('$')) {
                                continue;
                            }
                        }
                    }

                    let trimmed = content.trim();
                    if trimmed.starts_with('$') && !trimmed.eq_ignore_ascii_case("$this") {
                        issues.push(DocblockIssue {
                            message: "Missing docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                    }
                    continue;
                };

                if is_missing_docblock_type(type_str) {
                    if key == "return" {
                        issues.push(DocblockIssue {
                            message: "Missing return docblock type".to_string(),
                            start_offset,
                            end_offset: start_offset.saturating_add(1),
                        });
                        continue;
                    }

                    if key == "param" || key == "param-out" {
                        if let Some(var_name) =
                            crate::docblock::extract_var_name_from_content(content)
                        {
                            if typed_param_tags.contains(var_name.trim_start_matches('$')) {
                                continue;
                            }
                        }
                    }

                    issues.push(DocblockIssue {
                        message: "Missing docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                if (key == "param" || key == "param-out")
                    && crate::docblock::extract_var_name_from_content(content).is_none()
                {
                    issues.push(DocblockIssue {
                        message: "Invalid docblock type".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
                let resolved_union = self.resolve_docblock_union_type(
                    parsed_type,
                    self_class,
                    parent_class,
                    template_map,
                );
                let resolved_type = self.expand_docblock_class_constant_wildcards(
                    resolved_union,
                    self_class,
                    parent_class,
                    class_constants,
                );

                if union_has_invalid_class_string_targets(&resolved_type) {
                    issues.push(DocblockIssue {
                        message: "class-string param can only target object-like types".to_string(),
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                    continue;
                }

                if !self.is_valid_docblock_type_string(
                    type_str,
                    self_class,
                    parent_class,
                    template_map,
                    class_constants,
                ) {
                    issues.push(DocblockIssue {
                        message: if key == "return" {
                            "Invalid return docblock type".to_string()
                        } else {
                            "Invalid docblock type".to_string()
                        },
                        start_offset,
                        end_offset: start_offset.saturating_add(1),
                    });
                }
            }
        }
    }

    fn is_valid_docblock_type_string(
        &mut self,
        type_str: &str,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
        template_map: Option<&TemplateMap>,
        class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
    ) -> bool {
        if has_invalid_docblock_type_syntax(type_str) {
            return false;
        }

        if !has_balanced_type_delimiters(type_str) {
            return false;
        }

        if !has_valid_int_range_bounds(type_str) {
            return false;
        }

        if !has_valid_docblock_utility_type_arity(type_str) {
            return false;
        }

        if !has_valid_docblock_class_constant_syntax(type_str) {
            return false;
        }

        let parsed_type = crate::docblock::parse_type_string(type_str, self.interner);
        let resolved_union =
            self.resolve_docblock_union_type(parsed_type, self_class, parent_class, template_map);
        let resolved_type = self.expand_docblock_class_constant_wildcards(
            resolved_union,
            self_class,
            parent_class,
            class_constants,
        );
        union_has_valid_array_keys(&resolved_type) && !has_invalid_hyphenated_named_type(type_str)
    }

    fn is_docblock_override_method_visibility(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("override-method-visibility")
            || parsed.tags.contains_key("psalm-override-method-visibility")
    }

    fn is_docblock_override_property_visibility(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("override-property-visibility")
            || parsed
                .tags
                .contains_key("psalm-override-property-visibility")
    }

    fn is_docblock_readonly(&self, parsed: &crate::docblock::ParsedDocblock) -> bool {
        parsed.tags.contains_key("readonly")
            || parsed.tags.contains_key("psalm-readonly")
            || parsed.tags.contains_key("readonly-allow-private-mutation")
            || parsed
                .tags
                .contains_key("psalm-readonly-allow-private-mutation")
    }

    fn is_docblock_readonly_allow_private_mutation(
        &self,
        parsed: &crate::docblock::ParsedDocblock,
    ) -> bool {
        parsed.tags.contains_key("readonly-allow-private-mutation")
            || parsed
                .tags
                .contains_key("psalm-readonly-allow-private-mutation")
            || parsed.tags.contains_key("allow-private-mutation")
            || parsed.tags.contains_key("psalm-allow-private-mutation")
    }

    fn add_old_style_constructor_alias(&self, class_info: &mut ClassLikeInfo) {
        if class_info.methods.contains_key(&StrId::CONSTRUCT) {
            return;
        }

        let class_name = self.interner.lookup(class_info.name);
        if class_name.contains('\\') {
            return;
        }

        let class_name_lc = class_name.to_ascii_lowercase();
        let old_constructor_id = class_info.methods.keys().find_map(|method_id| {
            let method_name = self.interner.lookup(*method_id);
            if method_name.eq_ignore_ascii_case(class_name_lc.as_ref()) {
                Some(*method_id)
            } else {
                None
            }
        });

        let Some(old_constructor_id) = old_constructor_id else {
            return;
        };

        let Some(constructor_info) = class_info.methods.get(&old_constructor_id).cloned() else {
            return;
        };

        // Methods with explicit signature return types are normal methods in modern PHP.
        // Do not reinterpret them as old-style constructors.
        if constructor_info.signature_return_type.is_some() {
            return;
        }

        class_info
            .methods
            .insert(StrId::CONSTRUCT, constructor_info);
    }

    fn is_stub_file(&self) -> bool {
        let path = self.interner.lookup(self.file_path);
        path.starts_with("stubs/")
            || path.starts_with("stubs\\")
            || path.contains("/stubs/")
            || path.contains("\\stubs\\")
    }

    /// Resolve a type hint using the type resolver.
    fn resolve_type(
        &mut self,
        hint: &Hint<'_>,
        self_class: Option<StrId>,
        parent_class: Option<StrId>,
    ) -> TUnion {
        resolve_hint(
            hint,
            self.interner,
            self.current_namespace,
            self_class,
            parent_class,
            Some(&self.use_aliases),
            None,
        )
    }

    /// Create a fully qualified name from a local name.
    fn make_fqn(&mut self, local_name: &str) -> StrId {
        if let Some(ns) = self.current_namespace {
            let ns_str = self.interner.lookup(ns);
            let full_name = format!("{}\\{}", ns_str, local_name);
            self.interner.intern(&full_name)
        } else {
            self.interner.intern(local_name)
        }
    }

    /// Resolve an identifier to a fully qualified name.
    fn resolve_identifier(&mut self, ident: &Identifier<'_>) -> StrId {
        if ident.is_fully_qualified() {
            // Strip leading backslash
            let value = ident.value().strip_prefix('\\').unwrap_or(ident.value());
            self.interner.intern(value)
        } else {
            let value = ident.value();
            let (first_segment, remainder) = match value.split_once('\\') {
                Some((first, rest)) => (first, Some(rest)),
                None => (value, None),
            };

            if let Some(alias_target) = self.use_aliases.get(&first_segment.to_ascii_lowercase()) {
                if let Some(remainder) = remainder {
                    let alias_str = self.interner.lookup(*alias_target);
                    return self
                        .interner
                        .intern(&format!("{}\\{}", alias_str, remainder));
                }

                return *alias_target;
            }

            self.make_fqn(value)
        }
    }
}

// Helper functions that don't need self

#[derive(Default)]
struct ParsedFunctionAssertions {
    assertions: Vec<Assertion>,
    if_true_assertions: Vec<Assertion>,
    if_false_assertions: Vec<Assertion>,
}

fn extract_direct_var(expr: &Expression<'_>) -> Option<String> {
    match expr.unparenthesized() {
        Expression::Variable(variable) => match variable {
            mago_syntax::ast::ast::variable::Variable::Direct(direct) => {
                Some(direct.name.to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_null_expression(expr: &Expression<'_>) -> bool {
    matches!(
        expr.unparenthesized(),
        Expression::Literal(mago_syntax::ast::ast::literal::Literal::Null(_))
    )
}

fn normalize_use_name(name: &str) -> String {
    name.strip_prefix('\\').unwrap_or(name).to_string()
}

fn infer_param_default_type(
    expr: &Expression<'_>,
    interner: &Interner,
    self_class: Option<StrId>,
    class_constants: Option<&FxHashMap<StrId, ClassConstantInfo>>,
) -> Option<TUnion> {
    if let Some(inferred) = infer_simple_expression_type(expr) {
        return Some(inferred);
    }

    let class_constants = class_constants?;

    let Expression::Access(Access::ClassConstant(class_constant_access)) = expr.unparenthesized()
    else {
        return None;
    };

    let ClassLikeConstantSelector::Identifier(constant_name) = &class_constant_access.constant
    else {
        return None;
    };

    let is_current_class_reference = match class_constant_access.class.unparenthesized() {
        Expression::Self_(_) | Expression::Static(_) => true,
        Expression::Identifier(identifier) => self_class.is_some_and(|class_id| {
            let declared_class_name = interner.lookup(class_id);
            let declared_short_name = declared_class_name
                .rsplit('\\')
                .next()
                .unwrap_or(declared_class_name.as_ref());

            identifier
                .value()
                .eq_ignore_ascii_case(declared_class_name.as_ref())
                || identifier.value().eq_ignore_ascii_case(declared_short_name)
                || identifier
                    .value()
                    .trim_start_matches('\\')
                    .eq_ignore_ascii_case(declared_class_name.trim_start_matches('\\').as_ref())
        }),
        _ => false,
    };

    if !is_current_class_reference {
        return None;
    }

    let const_name = interner.intern(constant_name.value);
    class_constants
        .get(&const_name)
        .map(|const_info| const_info.constant_type.clone())
}

fn infer_simple_expression_type(expr: &Expression<'_>) -> Option<TUnion> {
    use mago_syntax::ast::ast::binary::BinaryOperator;
    use mago_syntax::ast::ast::literal::Literal;
    use mago_syntax::ast::ast::unary::UnaryPrefixOperator;

    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => {
            infer_simple_expression_type(parenthesized.expression)
        }
        Expression::Binary(binary) => match binary.operator {
            BinaryOperator::StringConcat(_) => Some(TUnion::string()),
            BinaryOperator::Division(_) => Some(TUnion::float()),
            BinaryOperator::Addition(_)
            | BinaryOperator::Subtraction(_)
            | BinaryOperator::Multiplication(_)
            | BinaryOperator::Modulo(_)
            | BinaryOperator::Exponentiation(_)
            | BinaryOperator::BitwiseAnd(_)
            | BinaryOperator::BitwiseOr(_)
            | BinaryOperator::BitwiseXor(_)
            | BinaryOperator::LeftShift(_)
            | BinaryOperator::RightShift(_) => Some(TUnion::int()),
            BinaryOperator::Equal(_)
            | BinaryOperator::NotEqual(_)
            | BinaryOperator::Identical(_)
            | BinaryOperator::NotIdentical(_)
            | BinaryOperator::AngledNotEqual(_)
            | BinaryOperator::LessThan(_)
            | BinaryOperator::LessThanOrEqual(_)
            | BinaryOperator::GreaterThan(_)
            | BinaryOperator::GreaterThanOrEqual(_)
            | BinaryOperator::Spaceship(_)
            | BinaryOperator::And(_)
            | BinaryOperator::Or(_)
            | BinaryOperator::LowAnd(_)
            | BinaryOperator::LowOr(_)
            | BinaryOperator::LowXor(_)
            | BinaryOperator::Instanceof(_) => Some(TUnion::bool()),
            BinaryOperator::NullCoalesce(_) => infer_simple_expression_type(binary.lhs)
                .or_else(|| infer_simple_expression_type(binary.rhs)),
        },
        Expression::UnaryPrefix(unary) => match &unary.operator {
            UnaryPrefixOperator::Plus(_) => infer_simple_expression_type(unary.operand),
            UnaryPrefixOperator::Negation(_) => {
                let operand_type = infer_simple_expression_type(unary.operand)?;
                Some(negate_simple_union(operand_type))
            }
            _ => None,
        },
        Expression::Literal(Literal::Null(_)) => Some(TUnion::null()),
        Expression::Literal(Literal::True(_)) => Some(TUnion::new(TAtomic::TTrue)),
        Expression::Literal(Literal::False(_)) => Some(TUnion::new(TAtomic::TFalse)),
        Expression::Literal(Literal::Integer(int_lit)) => int_lit
            .value
            .and_then(|value| i64::try_from(value).ok())
            .map(|value| TUnion::new(TAtomic::TLiteralInt { value }))
            .or_else(|| Some(TUnion::int())),
        Expression::Literal(Literal::Float(float_lit)) => {
            Some(TUnion::new(TAtomic::TLiteralFloat {
                value: float_lit.value.into_inner(),
            }))
        }
        Expression::Literal(Literal::String(string_lit)) => string_lit
            .value
            .map(|value| {
                TUnion::new(TAtomic::TLiteralString {
                    value: value.to_string(),
                })
            })
            .or_else(|| Some(TUnion::string())),
        Expression::Access(Access::ClassConstant(class_constant_access)) => {
            let ClassLikeConstantSelector::Identifier(constant_name) = &class_constant_access.constant
            else {
                return None;
            };

            if !constant_name.value.eq_ignore_ascii_case("class") {
                return None;
            }

            let Expression::Identifier(class_identifier) = class_constant_access.class.unparenthesized()
            else {
                return None;
            };

            Some(TUnion::new(TAtomic::TLiteralString {
                value: class_identifier.value().trim_start_matches('\\').to_string(),
            }))
        }
        Expression::Array(array) => infer_simple_array_type(array.elements.iter()),
        Expression::LegacyArray(array) => infer_simple_array_type(array.elements.iter()),
        _ => None,
    }
}

fn infer_simple_array_type<'a>(
    elements: impl Iterator<Item = &'a mago_syntax::ast::ast::array::ArrayElement<'a>>,
) -> Option<TUnion> {
    use mago_syntax::ast::ast::array::ArrayElement;
    use pzoom_code_info::t_atomic::ArrayKey;

    let mut properties = FxHashMap::default();
    let mut next_int_key = 0i64;
    let mut is_list = true;

    for element in elements {
        match element {
            ArrayElement::KeyValue(kv) => {
                let key_type = infer_simple_expression_type(kv.key)?;
                let value_type = infer_simple_expression_type(kv.value)?;
                let key = simple_union_to_array_key(&key_type)?;

                if !matches!(key, ArrayKey::Int(value) if value == next_int_key) {
                    is_list = false;
                }

                if let ArrayKey::Int(value) = key {
                    next_int_key = value + 1;
                    properties.insert(ArrayKey::Int(value), value_type);
                } else {
                    properties.insert(key, value_type);
                }
            }
            ArrayElement::Value(value) => {
                let value_type = infer_simple_expression_type(value.value)?;
                properties.insert(ArrayKey::Int(next_int_key), value_type);
                next_int_key += 1;
            }
            ArrayElement::Missing(_) => {}
            ArrayElement::Variadic(_) => return None,
        }
    }

    if properties.is_empty() {
        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::nothing()),
            value_type: Box::new(TUnion::nothing()),
        }));
    }

    Some(TUnion::new(TAtomic::TKeyedArray {
        properties,
        is_list,
        sealed: true,
        fallback_key_type: None,
        fallback_value_type: None,
    }))
}

fn simple_union_to_array_key(union: &TUnion) -> Option<pzoom_code_info::t_atomic::ArrayKey> {
    let single = union.get_single()?;

    match single {
        TAtomic::TLiteralInt { value } => Some(pzoom_code_info::t_atomic::ArrayKey::Int(*value)),
        TAtomic::TLiteralString { value } => value
            .parse::<i64>()
            .ok()
            .map(pzoom_code_info::t_atomic::ArrayKey::Int)
            .or_else(|| Some(pzoom_code_info::t_atomic::ArrayKey::String(value.clone()))),
        TAtomic::TNull => Some(pzoom_code_info::t_atomic::ArrayKey::String(String::new())),
        _ => None,
    }
}

fn negate_simple_union(t_union: TUnion) -> TUnion {
    if !t_union.is_single() {
        return t_union;
    }

    match t_union.get_single().cloned() {
        Some(TAtomic::TLiteralInt { value }) => TUnion::new(TAtomic::TLiteralInt { value: -value }),
        Some(TAtomic::TLiteralFloat { value }) => {
            TUnion::new(TAtomic::TLiteralFloat { value: -value })
        }
        Some(TAtomic::TInt) => TUnion::int(),
        Some(TAtomic::TFloat) => TUnion::float(),
        _ => t_union,
    }
}

fn extract_param_name_from_content(content: &str) -> Option<&str> {
    let mut depth: u32 = 0;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth = depth.saturating_sub(1),
            '$' if depth == 0 => {
                let start = idx + 1;
                let mut end = start;

                for (name_idx, name_ch) in content[start..].char_indices() {
                    if name_ch.is_ascii_alphanumeric() || name_ch == '_' {
                        end = start + name_idx + name_ch.len_utf8();
                    } else {
                        break;
                    }
                }

                if end > start {
                    return Some(&content[start..end]);
                }

                return None;
            }
            _ => {}
        }
    }

    None
}

fn split_docblock_method_params(params: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for ch in params.chars() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                let trimmed = current.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
                current.clear();
                continue;
            }
            _ => {}
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        parts.push(trimmed.to_string());
    }

    parts
}

fn find_docblock_method_signature_bounds(signature: &str) -> Option<(usize, usize)> {
    let mut stack = Vec::new();
    let mut pairs = Vec::new();

    for (idx, ch) in signature.char_indices() {
        match ch {
            '(' => stack.push(idx),
            ')' => {
                let open = stack.pop()?;
                pairs.push((open, idx));
            }
            _ => {}
        }
    }

    if !stack.is_empty() {
        return None;
    }

    for (open, close) in pairs.into_iter().rev() {
        let before_paren = signature[..open].trim();
        let Some((_, method_name)) = split_method_name(before_paren) else {
            continue;
        };

        if !is_valid_docblock_method_name(method_name) {
            continue;
        }

        let tail = signature[close + 1..].trim_start();
        if tail.contains(')') {
            continue;
        }

        return Some((open, close));
    }

    None
}

fn split_method_name(before_paren: &str) -> Option<(&str, &str)> {
    let mut method_start = None;

    for (idx, ch) in before_paren.char_indices().rev() {
        if ch.is_ascii_whitespace() {
            method_start = Some(idx + ch.len_utf8());
            break;
        }
    }

    let method_start = method_start.unwrap_or(0);
    let method_name = before_paren[method_start..].trim_start_matches('&').trim();
    if method_name.is_empty() {
        return None;
    }

    let return_part = before_paren[..method_start].trim();
    Some((return_part, method_name))
}

fn is_valid_docblock_method_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn take_first_docblock_type_token(content: &str) -> &str {
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;

    for (idx, ch) in content.char_indices() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            c if c.is_whitespace()
                && angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                return content[..idx].trim();
            }
            _ => {}
        }
    }

    content.trim()
}

fn resolve_key_of_template_union(union: &TUnion) -> TUnion {
    let mut key_union = TUnion::nothing();

    for atomic in &union.types {
        let atomic_key_union = resolve_key_of_template_atomic(atomic);
        key_union = if key_union.is_nothing() {
            atomic_key_union
        } else {
            combine_union_types(&key_union, &atomic_key_union, false)
        };
    }

    if key_union.is_nothing() {
        TUnion::array_key()
    } else {
        key_union
    }
}

fn resolve_key_of_template_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { key_type, .. }
        | TAtomic::TNonEmptyArray { key_type, .. }
        | TAtomic::TIterable { key_type, .. } => (**key_type).clone(),
        TAtomic::TList { .. } | TAtomic::TNonEmptyList { .. } => TUnion::int(),
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            ..
        } => {
            let mut key_union = fallback_key_type
                .as_ref()
                .map(|key_type| (**key_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for key in properties.keys() {
                let key_atomic = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TAtomic::TLiteralInt { value: *value }
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value) => TAtomic::TLiteralString {
                        value: value.clone(),
                    },
                };

                key_union = if key_union.is_nothing() {
                    TUnion::new(key_atomic)
                } else {
                    combine_union_types(&key_union, &TUnion::new(key_atomic), false)
                };
            }

            if key_union.is_nothing() {
                TUnion::array_key()
            } else {
                key_union
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_key_of_template_union(as_type),
        _ => TUnion::array_key(),
    }
}

fn resolve_value_of_template_union(union: &TUnion) -> TUnion {
    let mut value_union = TUnion::nothing();

    for atomic in &union.types {
        let atomic_value_union = resolve_value_of_template_atomic(atomic);
        value_union = if value_union.is_nothing() {
            atomic_value_union
        } else {
            combine_union_types(&value_union, &atomic_value_union, false)
        };
    }

    if value_union.is_nothing() {
        TUnion::mixed()
    } else {
        value_union
    }
}

fn resolve_value_of_template_atomic(atomic: &TAtomic) -> TUnion {
    match atomic {
        TAtomic::TArray { value_type, .. }
        | TAtomic::TNonEmptyArray { value_type, .. }
        | TAtomic::TIterable { value_type, .. }
        | TAtomic::TList { value_type }
        | TAtomic::TNonEmptyList { value_type } => (**value_type).clone(),
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            let mut value_union = fallback_value_type
                .as_ref()
                .map(|value_type| (**value_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for property_value in properties.values() {
                value_union = if value_union.is_nothing() {
                    property_value.clone()
                } else {
                    combine_union_types(&value_union, property_value, false)
                };
            }

            if value_union.is_nothing() {
                TUnion::mixed()
            } else {
                value_union
            }
        }
        TAtomic::TTemplateParam { as_type, .. } => resolve_value_of_template_union(as_type),
        _ => TUnion::mixed(),
    }
}

fn parse_template_tag_content(content: &str) -> Option<(String, Option<String>)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let template_name = parts.next()?.trim_matches(',');
    if template_name.is_empty() {
        return None;
    }

    let remaining: Vec<&str> = parts.collect();
    if remaining.len() >= 2 {
        let modifier = remaining[0].to_ascii_lowercase();
        if modifier == "as" || modifier == "of" || modifier == "super" {
            let bound = remaining[1..].join(" ");
            if !bound.trim().is_empty() {
                return Some((template_name.to_string(), Some(bound)));
            }
        }
    }

    Some((template_name.to_string(), None))
}

fn parse_type_alias_tag_content(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (alias_name, type_definition) = trimmed.split_once('=')?;
    let alias_name = alias_name.trim();
    let type_definition = type_definition.trim();

    if alias_name.is_empty() || type_definition.is_empty() {
        return None;
    }

    Some((alias_name.to_string(), type_definition.to_string()))
}

fn is_missing_docblock_type(type_str: &str) -> bool {
    let trimmed = type_str.trim();
    trimmed.starts_with('$') && !trimmed.eq_ignore_ascii_case("$this")
}

fn has_valid_docblock_utility_type_arity(type_str: &str) -> bool {
    const ONE_PARAM_UTILITIES: [&str; 6] = [
        "properties-of",
        "public-properties-of",
        "protected-properties-of",
        "private-properties-of",
        "int-mask",
        "int-mask-of",
    ];

    for utility in ONE_PARAM_UTILITIES {
        let search = format!("{utility}<");
        let mut search_from = 0usize;
        let lower = type_str.to_ascii_lowercase();

        while let Some(found) = lower[search_from..].find(&search) {
            let open_idx = search_from + found + utility.len();
            let Some(close_idx) = find_matching_angle_bracket(type_str, open_idx) else {
                return false;
            };

            let params = &type_str[open_idx + 1..close_idx];
            if count_top_level_generic_params(params) != 1 {
                return false;
            }

            search_from = close_idx + 1;
        }
    }

    true
}

fn find_matching_angle_bracket(input: &str, open_idx: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (idx, ch) in input[open_idx..].char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_idx + idx);
                }
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }

    None
}

fn count_top_level_generic_params(params: &str) -> usize {
    let trimmed = params.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut count = 1usize;

    for ch in params.chars() {
        match ch {
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            ',' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                count += 1;
            }
            _ => {}
        }
    }

    count
}

fn has_valid_docblock_class_constant_syntax(type_str: &str) -> bool {
    for part in split_docblock_union_parts(type_str) {
        if !class_constant_syntax_is_valid_in_part(part) {
            return false;
        }
    }

    true
}

fn class_constant_syntax_is_valid_in_part(part: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let bytes = part.as_bytes();
    let mut idx = 0usize;

    while idx + 1 < bytes.len() {
        let ch = bytes[idx] as char;

        if let Some(active_quote) = quote {
            if ch == '\\' && !escaped {
                escaped = true;
                idx += 1;
                continue;
            }

            if ch == active_quote && !escaped {
                quote = None;
            }

            escaped = false;
            idx += 1;
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            idx += 1;
            continue;
        }

        if ch == ':' && bytes[idx + 1] == b':' {
            let Some((class_part, const_part)) = extract_class_constant_parts(part, idx) else {
                return false;
            };

            if !is_valid_php_classlike_identifier(class_part) {
                return false;
            }

            if !const_part.eq_ignore_ascii_case("class") {
                if let Some(prefix) = const_part.strip_suffix('*') {
                    if !is_valid_php_const_identifier(prefix) {
                        return false;
                    }
                } else if !is_valid_php_const_identifier(const_part) {
                    return false;
                }
            }

            idx += 2;
            continue;
        }

        idx += 1;
    }

    true
}

fn extract_class_constant_parts(part: &str, separator_idx: usize) -> Option<(&str, &str)> {
    let left = part[..separator_idx].trim_end();
    let mut class_start = left.len();
    let left_bytes = left.as_bytes();

    while class_start > 0 {
        let ch = left_bytes[class_start - 1] as char;
        if is_class_name_char(ch) {
            class_start -= 1;
        } else {
            break;
        }
    }

    if class_start == left.len() {
        return None;
    }

    if class_start > 0 {
        let prev = left_bytes[class_start - 1] as char;
        if prev.is_ascii_alphanumeric() || prev == '_' || prev == '\\' || prev == '$' {
            return None;
        }
    }

    let class_part = &left[class_start..];
    if class_part.is_empty() || class_part.ends_with('\\') {
        return None;
    }

    let right = part[separator_idx + 2..].trim_start();
    let mut const_end = 0usize;
    for ch in right.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '*' {
            const_end += ch.len_utf8();
        } else {
            break;
        }
    }

    if const_end == 0 {
        return None;
    }

    let const_part = &right[..const_end];
    Some((class_part, const_part))
}

fn is_class_name_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '\\'
}

fn is_valid_php_classlike_identifier(name: &str) -> bool {
    let normalized = name.trim_start_matches('\\');
    if normalized.is_empty() {
        return false;
    }

    normalized
        .split('\\')
        .all(|segment| !segment.is_empty() && is_valid_php_const_identifier(segment))
}

fn split_docblock_union_parts(type_str: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut angle_depth = 0i32;
    let mut paren_depth = 0i32;
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (idx, ch) in type_str.char_indices() {
        if let Some(active_quote) = quote {
            if ch == '\\' && !escaped {
                escaped = true;
                continue;
            }

            if ch == active_quote && !escaped {
                quote = None;
            }

            escaped = false;
            continue;
        }

        match ch {
            '\'' | '"' => quote = Some(ch),
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '|' if angle_depth == 0
                && paren_depth == 0
                && brace_depth == 0
                && bracket_depth == 0 =>
            {
                parts.push(type_str[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }

    parts.push(type_str[start..].trim());
    parts
}

fn is_valid_php_const_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }

    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn has_invalid_hyphenated_named_type(type_str: &str) -> bool {
    const VALID_HYPHENATED_TYPE_TOKENS: [&str; 37] = [
        "non-empty-array",
        "non-empty-list",
        "non-empty-string",
        "non-empty-lowercase-string",
        "literal-string",
        "non-empty-literal-string",
        "non-empty-mixed",
        "numeric-string",
        "lowercase-string",
        "truthy-string",
        "non-falsy-string",
        "positive-int",
        "negative-int",
        "non-negative-int",
        "non-positive-int",
        "array-key",
        "key-of",
        "value-of",
        "properties-of",
        "public-properties-of",
        "protected-properties-of",
        "private-properties-of",
        "int-mask",
        "int-mask-of",
        "pure-callable",
        "pure-closure",
        "class-string-map",
        "class-string",
        "interface-string",
        "enum-string",
        "trait-string",
        "callable-string",
        "open-resource",
        "closed-resource",
        "no-return",
        "never-return",
        "never-returns",
    ];

    for part in split_docblock_union_parts(type_str) {
        let token = extract_docblock_base_type_token(part);

        if let Some(rest) = token.strip_prefix('-')
            && !rest.is_empty()
            && rest.chars().all(|ch| ch.is_ascii_digit())
        {
            continue;
        }

        if token.contains('-') && !VALID_HYPHENATED_TYPE_TOKENS.contains(&token.as_str()) {
            return true;
        }
    }

    false
}

fn extract_docblock_base_type_token(part: &str) -> String {
    let trimmed = part
        .trim()
        .trim_start_matches('?')
        .trim_start_matches('(')
        .trim_end_matches(')');

    let mut end = trimmed.len();
    for (idx, ch) in trimmed.char_indices() {
        if matches!(
            ch,
            '<' | '(' | '[' | '{' | ':' | '&' | '|' | ',' | ' ' | '\t' | '\n' | '\r'
        ) {
            end = idx;
            break;
        }
    }

    trimmed[..end].trim().to_ascii_lowercase()
}

fn has_invalid_docblock_type_syntax(type_str: &str) -> bool {
    let trimmed = type_str.trim();

    if trimmed.is_empty()
        || trimmed == "[]"
        || trimmed == "()"
        || trimmed.starts_with('[')
        || trimmed.starts_with('|')
        || trimmed.starts_with('&')
        || trimmed.ends_with('|')
        || trimmed.ends_with('&')
        || trimmed.ends_with(',')
        || trimmed.ends_with(':')
    {
        return true;
    }

    if trimmed.contains(';')
        || trimmed.contains("||")
        || trimmed.contains("&&")
        || trimmed.contains("|&")
        || trimmed.contains("&|")
        || trimmed.contains(",,")
        || trimmed.starts_with("\\?")
        || trimmed.contains("array(")
        || trimmed.contains("list(")
        || trimmed.contains(":}")
        || trimmed.contains(":]")
    {
        return true;
    }

    false
}

fn has_balanced_type_delimiters(type_definition: &str) -> bool {
    let mut angle_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;
    let mut bracket_depth: i32 = 0;
    let mut quote_char: Option<char> = None;
    let mut escaped = false;

    for ch in type_definition.chars() {
        if let Some(active_quote) = quote_char {
            if ch == '\\' && !escaped {
                escaped = true;
                continue;
            }

            if ch == active_quote && !escaped {
                quote_char = None;
            }

            escaped = false;
            continue;
        }

        match ch {
            '\'' | '"' => quote_char = Some(ch),
            '<' => angle_depth += 1,
            '>' => angle_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            _ => {}
        }

        if angle_depth < 0 || paren_depth < 0 || brace_depth < 0 || bracket_depth < 0 {
            return false;
        }
    }

    quote_char.is_none()
        && angle_depth == 0
        && paren_depth == 0
        && brace_depth == 0
        && bracket_depth == 0
}

fn union_has_valid_array_keys(union: &TUnion) -> bool {
    union.types.iter().all(atomic_has_valid_array_keys)
}

fn atomic_has_valid_array_keys(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        }
        | TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        }
        | TAtomic::TIterable {
            key_type,
            value_type,
        } => union_is_valid_array_key(key_type) && union_has_valid_array_keys(value_type),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_has_valid_array_keys(value_type)
        }
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().all(union_has_valid_array_keys),
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => atomic_has_valid_array_keys(as_type),
        TAtomic::TTemplateParam { as_type, .. } => union_has_valid_array_keys(as_type),
        _ => true,
    }
}

fn union_is_valid_array_key(union: &TUnion) -> bool {
    union.types.iter().all(|atomic| match atomic {
        TAtomic::TArrayKey
        | TAtomic::TInt
        | TAtomic::TPositiveInt
        | TAtomic::TNegativeInt
        | TAtomic::TIntRange { .. }
        | TAtomic::TString
        | TAtomic::TNumericString
        | TAtomic::TLowercaseString
        | TAtomic::TNonEmptyString
        | TAtomic::TNonEmptyLowercaseString
        | TAtomic::TClassString { .. }
        | TAtomic::TLiteralClassString { .. }
        | TAtomic::TLiteralInt { .. }
        | TAtomic::TLiteralString { .. }
        | TAtomic::TMixed
        | TAtomic::TNonEmptyMixed
        | TAtomic::TNothing
        | TAtomic::TTemplateParam { .. }
        | TAtomic::TTemplateParamClass { .. } => true,
        TAtomic::TNamedObject { .. } => true,
        // Psalm tolerates these in docblocks and reports access issues later.
        TAtomic::TArray { .. }
        | TAtomic::TNonEmptyArray { .. }
        | TAtomic::TList { .. }
        | TAtomic::TNonEmptyList { .. }
        | TAtomic::TKeyedArray { .. } => true,
        _ => false,
    })
}

fn union_has_invalid_class_string_targets(union: &TUnion) -> bool {
    union
        .types
        .iter()
        .any(atomic_has_invalid_class_string_target)
}

fn atomic_has_invalid_class_string_target(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => class_string_target_is_explicitly_invalid(as_type),
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params
            .iter()
            .any(union_has_invalid_class_string_targets),
        TAtomic::TTemplateParam { as_type, .. } => union_has_invalid_class_string_targets(as_type),
        TAtomic::TObjectIntersection { types } => {
            types.iter().any(atomic_has_invalid_class_string_target)
        }
        _ => false,
    }
}

fn class_string_target_is_explicitly_invalid(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => true,
        TAtomic::TTemplateParam { as_type, .. } => as_type
            .types
            .iter()
            .all(class_string_target_is_explicitly_invalid),
        TAtomic::TObjectIntersection { types } => {
            let has_object_like = types.iter().any(|inner| {
                matches!(
                    inner,
                    TAtomic::TObject
                        | TAtomic::TNamedObject { .. }
                        | TAtomic::TTemplateParamClass { .. }
                        | TAtomic::TLiteralClassString { .. }
                )
            });
            let has_callable_like = types
                .iter()
                .any(|inner| matches!(inner, TAtomic::TCallable { .. } | TAtomic::TClosure { .. }));

            has_callable_like && !has_object_like
        }
        _ => false,
    }
}

fn has_valid_int_range_bounds(type_str: &str) -> bool {
    let lower = type_str.to_ascii_lowercase();
    let mut offset = 0usize;

    while let Some(found) = lower[offset..].find("int<") {
        let int_start = offset + found;
        if int_start > 0 {
            let previous = lower.as_bytes()[int_start - 1];
            if previous.is_ascii_alphanumeric() || previous == b'_' || previous == b'\\' {
                offset = int_start + 4;
                continue;
            }
        }

        let range_start = int_start + 4;
        let mut depth = 1i32;
        let mut range_end: Option<usize> = None;

        for (idx, ch) in type_str[range_start..].char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        range_end = Some(range_start + idx);
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(range_end) = range_end else {
            return false;
        };

        if !is_valid_single_int_range(&type_str[range_start..range_end]) {
            return false;
        }

        offset = range_end + 1;
    }

    true
}

fn is_valid_single_int_range(range_content: &str) -> bool {
    let parts: Vec<&str> = range_content.split(',').map(str::trim).collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return false;
    }

    let Some(lower_bound) = parse_int_range_bound(parts[0], true) else {
        return false;
    };
    let Some(upper_bound) = parse_int_range_bound(parts[1], false) else {
        return false;
    };

    if let (Some(min), Some(max)) = (lower_bound, upper_bound) {
        return min <= max;
    }

    true
}

fn parse_int_range_bound(bound: &str, is_lower_bound: bool) -> Option<Option<i64>> {
    let lowered = bound.to_ascii_lowercase();
    if lowered == "min" {
        return if is_lower_bound { Some(None) } else { None };
    }
    if lowered == "max" {
        return if is_lower_bound { None } else { Some(None) };
    }

    bound.parse::<i64>().ok().map(Some)
}

fn parse_import_type_tag_content(content: &str) -> Option<(String, String, String)> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.len() < 3 || !parts[1].eq_ignore_ascii_case("from") {
        return None;
    }

    let imported_alias = parts[0].trim();
    let source_name = parts[2].trim();
    if imported_alias.is_empty() || source_name.is_empty() {
        return None;
    }

    let alias_name = if parts.len() >= 5 && parts[3].eq_ignore_ascii_case("as") {
        parts[4].trim()
    } else {
        imported_alias
    };

    if alias_name.is_empty() {
        return None;
    }

    Some((
        imported_alias.to_string(),
        source_name.to_string(),
        alias_name.to_string(),
    ))
}

fn parse_method_modifiers(
    modifiers: &Sequence<'_, Modifier<'_>>,
) -> (Visibility, bool, bool, bool) {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_abstract = false;
    let mut is_final = false;

    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => visibility = Visibility::Public,
            Modifier::Protected(_) => visibility = Visibility::Protected,
            Modifier::Private(_) => visibility = Visibility::Private,
            Modifier::Static(_) => is_static = true,
            Modifier::Abstract(_) => is_abstract = true,
            Modifier::Final(_) => is_final = true,
            _ => {}
        }
    }

    (visibility, is_static, is_abstract, is_final)
}

fn parse_visibility_modifier(modifier: &Modifier<'_>) -> Option<Visibility> {
    match modifier {
        Modifier::Public(_) => Some(Visibility::Public),
        Modifier::Protected(_) => Some(Visibility::Protected),
        Modifier::Private(_) => Some(Visibility::Private),
        _ => None,
    }
}

fn parse_property_modifiers(modifiers: &Sequence<'_, Modifier<'_>>) -> (Visibility, bool, bool) {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_readonly = false;

    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => visibility = Visibility::Public,
            Modifier::Protected(_) => visibility = Visibility::Protected,
            Modifier::Private(_) => visibility = Visibility::Private,
            Modifier::Static(_) => is_static = true,
            Modifier::Readonly(_) => is_readonly = true,
            _ => {}
        }
    }

    (visibility, is_static, is_readonly)
}

fn parse_const_visibility(modifiers: &Sequence<'_, Modifier<'_>>) -> Visibility {
    for modifier in modifiers {
        match modifier {
            Modifier::Public(_) => return Visibility::Public,
            Modifier::Protected(_) => return Visibility::Protected,
            Modifier::Private(_) => return Visibility::Private,
            _ => {}
        }
    }
    Visibility::Public
}
