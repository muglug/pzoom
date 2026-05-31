//! Function declaration scanning.
//!
//! Mirrors Hakana's `code_info_builder/functionlike_scanner.rs`. This method belongs
//! to [`DeclarationCollector`]; split out of the module root for organization.

use mago_span::HasSpan;
use mago_syntax::ast::ast::function_like::function::Function;

use pzoom_code_info::class_like_info::DocblockIssue;
use pzoom_code_info::functionlike_info::{
    FunctionLikeInfo,
    FunctionTemplateType,
};
use pzoom_code_info::{TAtomic, TUnion};
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
        let mut is_pure = false;
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

        if let Some(docblock) = self.find_preceding_docblock(span.start.offset) {
            let parsed = crate::docblock::parse(docblock, 0);
            let template_bindings = self.parse_docblock_template_bindings(
                &parsed,
                name,
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
                    defining_entity: binding.defining_entity,
                    as_type: binding.as_type.clone(),
                })
                .collect();

            function_template_map = self.build_template_map_from_bindings(&template_bindings, None);
            let function_param_names: Vec<StrId> =
                params.iter().map(|param| param.name).collect();
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
                    Some(conditional) => {
                        TUnion::new(TAtomic::TConditional(Box::new(conditional)))
                    }
                    None => docblock_return,
                });
            }
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
            if_this_is_type,
            docblock_issues: function_docblock_issues,
            inherits_docblock,
            no_named_arguments,
            defined_constants,
            ..Default::default()
        };

        self.declarations.functions.push(info);
    }
}
