//! Function declaration scanning.
//!
//! Mirrors Hakana's `code_info_builder/functionlike_scanner.rs`. This method belongs
//! to [`DeclarationCollector`]; split out of the module root for organization.

use mago_span::HasSpan;
use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::GenericParent;
use pzoom_code_info::TUnion;
use pzoom_code_info::class_like_info::DocblockIssue;
use pzoom_code_info::functionlike_info::{FunctionLikeInfo, FunctionTemplateType};
use pzoom_str::StrId;
use rustc_hash::FxHashMap;

use super::{DeclarationCollector, TemplateMap};

impl<'a, 'p> DeclarationCollector<'a, 'p> {
    pub(crate) fn visit_function(&mut self, func: &Function<'_>) {
        let name = self.make_fqn(func.name.value);
        let span = func.span();

        let mut signature_return_type = func
            .return_type_hint
            .as_ref()
            .map(|rth| self.resolve_type(&rth.hint, None, None));

        let mut params =
            self.collect_params(&func.parameter_list.parameters, None, None, None, None);

        // `return_type` holds the docblock type only (Psalm's model); the native hint
        // stays in `signature_return_type`. Effective reads use get_return_type().
        let mut return_type = None;
        // The native hint's span is the fallback origin location; a docblock
        // @return (captured below) takes precedence, matching Psalm's
        // return_type_location.
        let mut return_type_location: Option<(u32, u32)> =
            func.return_type_hint.as_ref().map(|rth| {
                let hint_span = rth.hint.span();
                (hint_span.start.offset, hint_span.end.offset)
            });
        let mut is_pure = false;
        let mut has_throws = false;
        let mut unused_docblock_params: Vec<(String, u32)> = Vec::new();
        let mut is_mutation_free = false;
        let mut is_deprecated = false;
        let mut deprecation_message = None;
        let mut internal = Vec::new();
        let mut assertions = Vec::new();
        let mut if_true_assertions = Vec::new();
        let mut if_false_assertions = Vec::new();
        let mut template_types = Vec::new();
        let mut if_this_is_type = None;
        let mut inherits_docblock = false;
        let mut no_named_arguments = false;
        let mut function_template_map: TemplateMap = FxHashMap::default();
        let mut function_docblock_issues: Vec<DocblockIssue> = Vec::new();
        let mut taints = pzoom_code_info::functionlike_info::FunctionLikeTaints::default();
        let mut global_types: Vec<(StrId, TUnion)> = Vec::new();

        if let Some((docblock_start, docblock)) =
            self.find_preceding_docblock_with_offset(span.start.offset)
        {
            let parsed = crate::docblock::parse(docblock, 0);
            let template_bindings = self.parse_docblock_template_bindings(
                &parsed,
                GenericParent::FunctionLike(name),
                None,
                None,
                None,
                None,
                &mut function_docblock_issues,
            );
            template_types = template_bindings
                .iter()
                .map(|binding| FunctionTemplateType {
                    name: binding.name,
                    conditional_subject: false,
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                })
                .collect();

            function_template_map = self.build_template_map_from_bindings(&template_bindings, None);

            // `@global Type $var` declarations type the matching `global $var;`
            // imports in the body (Psalm's FunctionLikeStorage::$global_types).
            if let Some(global_tags) = parsed.tags.get("global") {
                for content in global_tags.values() {
                    let (Some(type_str), Some(var_name)) = (
                        crate::docblock::extract_type_string_from_content(content),
                        crate::docblock::extract_var_name_from_content(content),
                    ) else {
                        continue;
                    };
                    let parsed_type =
                        crate::docblock::parse_type_string(type_str, self.interner.parent_ref())
                            .unwrap_or_else(|_| TUnion::mixed());
                    let resolved_type = self.resolve_docblock_union_type(
                        parsed_type,
                        None,
                        None,
                        Some(&function_template_map),
                    );
                    global_types.push((self.interner.intern(var_name), resolved_type));
                }
            }

            let function_param_names: Vec<StrId> = params.iter().map(|param| param.name).collect();
            self.validate_function_docblock_type_tags(
                &parsed,
                span.start.offset,
                None,
                None,
                Some(&function_template_map),
                None,
                &function_param_names,
                &mut function_docblock_issues,
            );
            inherits_docblock = self.is_docblock_inheritdoc(&parsed);
            is_pure = self.is_docblock_pure(&parsed);
            has_throws =
                parsed.tags.contains_key("throws") || parsed.tags.contains_key("phpstan-throws");
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

            let (unmatched_param_tags, has_undertyped_params) = self.apply_docblock_param_types(
                &parsed,
                &mut params,
                None,
                None,
                Some(&function_template_map),
                None,
            );
            if has_undertyped_params {
                for (tag_name, tag_offset) in unmatched_param_tags {
                    function_docblock_issues.push(DocblockIssue {
                        message: format!(
                            "Incorrect param name ${} in docblock for {}",
                            tag_name,
                            self.interner.lookup(name)
                        ),
                        start_offset: tag_offset,
                        end_offset: tag_offset.saturating_add(1),
                    });
                }
            } else {
                unused_docblock_params = unmatched_param_tags;
            }
            // Conditional-type subjects (`$param is …`, `func_num_args()`,
            // PHP version tokens) parse against this function's scope and
            // register generated templates on it (Psalm's
            // FunctionLikeDocblockScanner model).
            self.conditional_subject_scope = super::ConditionalSubjectScope {
                entity: Some(GenericParent::FunctionLike(name)),
                params: params
                    .iter()
                    .map(|param| (param.name, param.get_type().cloned()))
                    .collect(),
                generated_templates: Vec::new(),
                subject_names: Vec::new(),
            };
            self.apply_docblock_param_out_types(
                &parsed,
                &mut params,
                None,
                None,
                Some(&function_template_map),
                None,
            );
            if let Some((docblock_return, docblock_conditional_return)) = self
                .get_docblock_return_type(
                    &parsed,
                    None,
                    None,
                    Some(&function_template_map),
                    None,
                    &function_param_names,
                )
            {
                return_type = Some(match docblock_conditional_return {
                    Some(conditional) => super::docblock_conditional_union(conditional),
                    None => docblock_return,
                });
                return_type_location =
                    parsed
                        .get_return_with_offset()
                        .and_then(|(offset, content)| {
                            crate::docblock::extract_type_string_from_content(content).map(
                                |type_str| {
                                    (
                                        docblock_start + offset as u32,
                                        docblock_start + (offset + type_str.len()) as u32,
                                    )
                                },
                            )
                        });
            }
            // `@psalm-taint-escape (<conditional>)` parses while the
            // conditional-subject scope is alive (its `$param is …` subject
            // resolves against this function's params).
            let mut conditional_taint_escapes = self.parse_conditional_taint_escapes(
                &parsed,
                None,
                None,
                Some(&function_template_map),
                None,
            );

            // Docblock assertions parse while the conditional-subject scope is
            // alive too: a conditional assertion type (`@psalm-assert-if-true
            // =(T is '' ? ...)`) registers its subject template so call sites
            // keep literal bounds for it.
            let parsed_assertions =
                self.get_docblock_assertions(&parsed, None, None, Some(&function_template_map));
            assertions.extend(parsed_assertions.assertions);
            if_true_assertions.extend(parsed_assertions.if_true_assertions);
            if_false_assertions.extend(parsed_assertions.if_false_assertions);

            let generated_conditional_templates =
                std::mem::take(&mut self.conditional_subject_scope.generated_templates);
            template_types.extend(generated_conditional_templates);
            for template_type in &mut template_types {
                if self
                    .conditional_subject_scope
                    .subject_names
                    .contains(&template_type.name)
                {
                    template_type.conditional_subject = true;
                }
            }
            self.conditional_subject_scope = super::ConditionalSubjectScope::default();
            if_this_is_type = self.get_docblock_if_this_is_type(
                &parsed,
                None,
                None,
                Some(&function_template_map),
                None,
            );

            // Apply ignore-nullable/falsable to the effective return type: the docblock
            // type if present, otherwise the native signature type.
            if let Some(return_type) = return_type.as_mut().or(signature_return_type.as_mut()) {
                if self.is_docblock_ignore_nullable_return(&parsed) {
                    return_type.ignore_nullable_issues = true;
                }
                if self.is_docblock_ignore_falsable_return(&parsed) {
                    return_type.ignore_falsable_issues = true;
                }
            }

            super::clear_docblock_flag_when_signature_backed(
                return_type.as_mut(),
                signature_return_type.as_ref(),
            );

            let (scanned_taints, _raw_conditional_escapes) =
                self.scan_docblock_taints(&parsed, &mut params, is_pure);
            taints = scanned_taints;
            taints.conditionally_removed_taints = std::mem::take(&mut conditional_taint_escapes);

            self.active_docblock_type_aliases = previous_aliases;
        }

        // JetBrains' #[Pure] attribute (phpstorm-stubs): bare #[Pure] matches
        // @psalm-pure (no global-scope dependence), #[Pure(true)] matches
        // @pure (result may depend on global scope); both map to is_pure.
        is_pure = is_pure || self.has_attribute_named(&func.attribute_lists, "Pure");

        // Builtin sinks (Psalm's InternalTaintSinkMap) are looked up at call
        // time (argument_analyzer::get_builtin_argument_taints), mirroring
        // Hakana's get_argument_taints.
        //
        // Hakana specializes every plain function's taint nodes per call site
        // (`functionlike_scanner`: no `$this` ⇒ `specialize_call`); only
        // methods stay global unless pure/annotated.
        taints.specialize_call = true;

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

        // Body-derived implicit assertions (a pzoom inference Psalm doesn't do)
        // must not double up with an explicit docblock assertion for the same
        // var: the duplicate would re-assert an already-narrowed type and
        // produce a false RedundantCondition.
        let explicit_assertion_vars: rustc_hash::FxHashSet<_> = assertions
            .iter()
            .map(|assertion| assertion.var_id)
            .collect();
        assertions.extend(
            self.get_implicit_assertions(func.body.statements.as_slice(), None, None)
                .into_iter()
                .filter(|assertion| !explicit_assertion_vars.contains(&assertion.var_id)),
        );
        let defined_constants = self.collect_defined_constants_from_statements(
            func.body.statements.as_slice(),
            None,
            None,
        );
        let has_variadic_param = params.iter().any(|param| param.is_variadic);

        let declared_if_not_exists = {
            let short_name = func.name.value.to_ascii_lowercase();
            self.current_not_exists_function_guards
                .contains(&short_name)
        };
        let info = FunctionLikeInfo {
            declared_if_not_exists,
            name,
            params,
            global_types,
            return_type,
            return_type_location,
            name_location: {
                let name_span = mago_span::HasSpan::span(&func.name);
                Some((name_span.start.offset, name_span.end.offset))
            },
            signature_return_type,
            is_pure,
            has_throws,
            unused_docblock_params,
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
            if_this_is_type,
            docblock_issues: function_docblock_issues,
            inherits_docblock,
            no_named_arguments,
            defined_constants,
            taints,
            ..Default::default()
        };

        self.declarations.functions.push(info);

        // Class-likes declared inside a function body are file-scoped once
        // the function runs; Psalm's ReflectorVisitor collects them too
        // (`function f() { class Foo {} ... new $d; }`).
        for stmt in func.body.statements.iter() {
            if matches!(
                stmt,
                mago_syntax::ast::ast::statement::Statement::Class(_)
                    | mago_syntax::ast::ast::statement::Statement::Interface(_)
                    | mago_syntax::ast::ast::statement::Statement::Trait(_)
                    | mago_syntax::ast::ast::statement::Statement::Enum(_)
            ) {
                self.visit_statement(stmt);
            }
        }
    }
}
