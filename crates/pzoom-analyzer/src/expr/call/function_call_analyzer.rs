//! Function call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{
    FunctionLikeIdentifier, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_code_info::VarName;
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template;

pub(crate) use super::callable_validation::{
    analyze_arguments_with_callable_context, callable_union_is_pure, infer_array_map_callable_return_type,
    infer_invokable_object_return_type, maybe_check_builtin_callable_arity,
    resolve_callable_union_for_template_inference, union_contains_non_pure_callable,
    validate_direct_callable_invocation, widen_literal_scalar_union_for_callable,
};
pub(crate) use super::function_call_return_type_fetcher::resolve_functionlike_return_type;
pub(crate) use super::arguments_analyzer::{
    apply_param_out_types, is_array_filter_function_name, predeclare_by_ref_argument_vars,
    verify_arguments,
};
pub(crate) use super::function_call_assertion_analyzer::{
    apply_assert_builtin_assertions, apply_post_call_assertions,
    emit_non_mutation_free_magic_property_assertion_issues, narrow_union_to_truthy,
};
use super::{
    argument_analyzer, callable_validation,
    function_call_return_type_fetcher, named_function_call_handler,
};
use pzoom_code_info::TemplateResult;
use std::rc::Rc;

/// Analyze a function call expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    let enforce_mutation_free = is_mutation_free_context(analyzer);

    // Analyze the callee expression to get the function name
    // (general use — Hakana's expression_call_analyzer; `$c(22)` uses `$c`).
    let was_inside_general_use = context.inside_general_use;
    context.inside_general_use = true;
    let callee_pos =
        expression_analyzer::analyze(analyzer, func_call.function, analysis_data, context);
    context.inside_general_use = was_inside_general_use;

    // Try to get the function name and whether it's fully qualified.
    // We need this before analyzing arguments to predeclare by-ref out variables.
    let (func_name, is_fq, name_offset) = get_function_name(func_call.function);

    // Psalm's FunctionCallAnalyzer: a dynamic call `$name()` whose callee is a
    // variable/expression (no literal function name) is an INPUT_CALLABLE taint
    // sink — tainted text used as the invoked function name is a
    // TaintedCallable. Connect the callee's dataflow to a `variable-call` sink.
    if analyzer.config.taint_analysis
        && func_name.is_none()
        && let Some(callee_type) = analysis_data.expr_types.get(&callee_pos).cloned()
        && !callee_type.parent_nodes.is_empty()
    {
        let callee_span = mago_span::HasSpan::span(func_call.function);
        crate::expr::output_constructs::add_construct_argument_dataflow(
            analyzer,
            "variable-call",
            &[pzoom_code_info::data_flow::node::SinkType::Callable],
            0,
            callee_pos,
            &callee_type,
            (callee_span.start.offset, callee_span.end.offset),
            analysis_data,
            context,
        );
    }

    // func_get_args() implicitly reads every parameter of the enclosing
    // function-like (Psalm skips unused-param reporting for such bodies).
    if func_name.is_some_and(|name| name.eq_ignore_ascii_case("func_get_args"))
        && let Some(function_info) = analyzer.function_info
    {
        analysis_data
            .func_get_args_functions
            .insert(function_info.start_offset);
    }
    let pre_resolved_func_info =
        func_name.and_then(|name| resolve_function(analyzer, name, is_fq, name_offset, context));
    predeclare_by_ref_argument_vars(
        analyzer,
        func_name,
        pre_resolved_func_info,
        &func_call.argument_list.arguments,
        context,
    );

    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    let arg_positions: Vec<Pos> = args
        .iter()
        .map(|arg| {
            // The VALUE expression's span — a named argument's own span
            // includes the `name:` prefix, where no type is stored.
            let span = mago_span::HasSpan::span(arg.value());
            (span.start.offset, span.end.offset)
        })
        .collect();

    // `X::class` arguments of the class_exists family (and class_alias, whose
    // alias name does not exist yet) are exempt from class existence checks —
    // Psalm's Context::inside_class_exists.
    let is_class_exists_like_call = func_name.is_some_and(|name| {
        name.eq_ignore_ascii_case("class_exists")
            || name.eq_ignore_ascii_case("interface_exists")
            || name.eq_ignore_ascii_case("enum_exists")
            || name.eq_ignore_ascii_case("trait_exists")
            || name.eq_ignore_ascii_case("class_alias")
    });
    let was_inside_class_exists = context.inside_class_exists;
    if is_class_exists_like_call {
        context.inside_class_exists = true;
    }

    if let Some(func_info) = pre_resolved_func_info {
        analyze_arguments_with_callable_context(
            analyzer,
            Some(func_info.name),
            &args,
            &arg_positions,
            &func_info.params,
            &get_template_defaults(func_info),
            analysis_data,
            context,
        );
    } else if let Some(callable_info) = analysis_data
        .expr_types
        .get(&callee_pos)
        .cloned()
        .and_then(|callee_type| {
            crate::expr::call::callable_validation::direct_callable_function_info(
                analyzer,
                &callee_type,
            )
        })
    {
        // A direct callable / invokable-object call: seed closure arguments
        // from the callable's own parameter signature.
        analyze_arguments_with_callable_context(
            analyzer,
            None,
            &args,
            &arg_positions,
            &callable_info.params,
            &pzoom_code_info::ttype::template::TemplateResult::default(),
            analysis_data,
            context,
        );
    } else {
        for arg in &args {
            argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        }
    }

    context.inside_class_exists = was_inside_class_exists;

    // Try to look up function return type
    if let Some(name) = func_name {
        if let Some(return_type) = named_function_call_handler::handle(
            analyzer,
            name,
            func_call,
            &arg_positions,
            pos,
            analysis_data,
            context,
        ) {
            let functionlike_id = pre_resolved_func_info
                .map(|info| FunctionLikeIdentifier::Function(info.name))
                .unwrap_or_else(|| {
                    FunctionLikeIdentifier::Function(analyzer.interner.intern(name))
                });
            let return_type = add_function_call_dataflow(
                analyzer,
                analysis_data,
                functionlike_id,
                pre_resolved_func_info,
                pos,
                &arg_positions,
                return_type,
            );
            analysis_data.expr_types.insert(pos, Rc::new(return_type));
            return;
        }

        // Resolve the function name considering namespace context
        let func_info = pre_resolved_func_info;
        // Psalm reports ForbiddenCode but keeps analyzing the call (its
        // arguments still flow into taint sinks — e.g. var_dump's
        // `@psalm-taint-sink html` with var_dump in forbiddenFunctions).
        if is_forbidden_function_call(analyzer, name, func_info) {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ForbiddenCode,
                format!("Cannot use forbidden function {}", name),
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }

        if let Some(func_info) = func_info {
            // Consult the params providers (Psalm's FunctionCallAnalyzer does
            // the same via `$codebase->functions->params_provider`): some
            // builtin calls have parameter lists that depend on the call
            // site, and a provider may also ask for the generic parameter
            // validation to be skipped entirely.
            let mut skip_param_validation = false;
            let mut provider_adjusted_info = None;
            if let Some(result) = crate::params_provider::dispatch_function_params(
                &crate::params_provider::FunctionParamsProviderEvent {
                    analyzer,
                    function_id: name,
                    args: &args,
                    arg_positions: &arg_positions,
                    context,
                },
                analysis_data,
            ) {
                match result {
                    crate::params_provider::FunctionParamsProviderResult::Params(params) => {
                        let mut adjusted = func_info.clone();
                        adjusted.params = params;
                        provider_adjusted_info = Some(adjusted);
                    }
                    crate::params_provider::FunctionParamsProviderResult::SkipValidation => {
                        skip_param_validation = true;
                    }
                }
            }
            let func_info = provider_adjusted_info.as_ref().unwrap_or(func_info);

            let is_class_alias_call = func_info.name == StrId::CLASS_ALIAS;
            apply_function_defined_constants_side_effects(func_info, context);
            let mut template_result = get_template_defaults(func_info);
            infer_function_template_replacements(
                analyzer,
                func_call,
                &arg_positions,
                func_info,
                &mut template_result,
                analysis_data,
                context,
            );

            // Psalm's HighOrderFunctionArgHandler: an argument that is itself
            // a call returning a templated callable (`filter(fn($i) => ...)`
            // passed to `pipe(...)`) solves its templates against the outer
            // expectation, and its nested closures re-analyze with the solved
            // param types — then the outer templates re-infer from the
            // refined callables.
            if reanalyze_high_order_call_args(
                analyzer,
                &args,
                func_info,
                &template_result,
                analysis_data,
                context,
            ) {
                template_result = get_template_defaults(func_info);
                infer_function_template_replacements(
                    analyzer,
                    func_call,
                    &arg_positions,
                    func_info,
                    &mut template_result,
                    analysis_data,
                    context,
                );
            }

            record_named_function_callsite_argument_types(
                func_info.name,
                &arg_positions,
                analysis_data,
            );

            // Check for deprecated functions
            let is_stub_function = analyzer
                .codebase
                .files
                .get(&func_info.file_path)
                .is_some_and(|file_info| file_info.is_stub);

            if func_info.is_deprecated && !is_stub_function {
                let (line, col) = analyzer.get_line_column(pos.0);
                let message = func_info
                    .deprecation_message
                    .as_ref()
                    .map(|m| format!("Function {} is deprecated: {}", name, m))
                    .unwrap_or_else(|| format!("Function {} is deprecated", name));
                analysis_data.add_issue(Issue::new(
                    IssueKind::DeprecatedFunction,
                    message,
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if !can_access_internal(analyzer, &func_info.internal, Some(context)) {
                let (line, col) = analyzer.get_line_column(pos.0);
                let scope_phrase = format_internal_scope_phrase(analyzer, &func_info.internal);
                analysis_data.add_issue(Issue::new(
                    IssueKind::InternalMethod,
                    format!("Function {} is internal to {}", name, scope_phrase),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            // Verify argument types against function parameters
            if !is_class_alias_call && !skip_param_validation {
                verify_arguments(
                    analyzer,
                    func_call,
                    &arg_positions,
                    func_info,
                    name,
                    analysis_data,
                    context,
                    &template_result,
                    pos,
                );
            }

            let fetched_return_type = function_call_return_type_fetcher::fetch(
                analyzer,
                name,
                Some(func_info),
                &args,
                &arg_positions,
                analysis_data,
                context,
                Some(&template_result),
            );

            if !skip_param_validation {
                apply_param_out_types(
                    analyzer,
                    func_info.name,
                    &func_info.template_types,
                    &args,
                    &arg_positions,
                    &func_info.params,
                    analysis_data,
                    context,
                    &template_result,
                    pos,
                );
            }

            apply_assert_builtin_assertions(
                analyzer,
                func_info.name,
                func_call,
                analysis_data,
                context,
            );
            // assert() is fully handled by the builtin formula path above;
            // re-applying its stubbed `@psalm-assert truthy` would re-assert
            // the already-narrowed type and report a false RedundantCondition.
            if func_info.name != StrId::ASSERT {
                apply_post_call_assertions(
                    analyzer,
                    func_call,
                    func_info,
                    context,
                    &template_result,
                    analysis_data,
                );
            }
            emit_non_mutation_free_magic_property_assertion_issues(
                analyzer,
                func_info,
                func_call,
                analysis_data,
            );

            if enforce_mutation_free
                && !function_call_is_mutation_free(
                    analyzer,
                    func_info,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                )
            {
                // Psalm points at the function name node.
                let name_span = func_call.function.span();
                let (line, col) = analyzer.get_line_column(name_span.start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureFunctionCall,
                    format!(
                        "Cannot call an impure function {} from a mutation-free context",
                        name
                    ),
                    analyzer.file_path,
                    name_span.start.offset,
                    name_span.end.offset,
                    line,
                    col,
                ));
            }

            check_unused_function_call(
                analyzer,
                func_info,
                name,
                func_call,
                &args,
                &arg_positions,
                analysis_data,
                context,
                pos,
            );

            // A function-like with no declared (or fetchable) return type
            // still needs call dataflow in whole-program mode: Psalm taints
            // the call's mixed result so `echo rawinput();` flows.
            let fetched_return_type = fetched_return_type.or_else(|| {
                matches!(
                    analysis_data.data_flow_graph.kind,
                    pzoom_code_info::GraphKind::WholeProgram(_)
                )
                .then(TUnion::mixed)
            });

            if let Some(resolved_return_type) = fetched_return_type {
                let mut resolved_return_type = add_function_call_dataflow(
                    analyzer,
                    analysis_data,
                    FunctionLikeIdentifier::Function(func_info.name),
                    Some(func_info),
                    pos,
                    &arg_positions,
                    resolved_return_type,
                );
                // Psalm marks a pure function call 'pure' when its value is
                // used; a method called on that value is then pure-compatible
                // (MethodCallPurityAnalyzer's receiver attribute check).
                if func_info.is_pure {
                    resolved_return_type = resolved_return_type.with_reference_free(true);
                }
                analysis_data.expr_types.insert(pos, Rc::new(resolved_return_type));
                return;
            }
        } else {
            // Function not found in codebase
            // Don't emit error for language constructs that look like functions
            if !is_language_construct(name)
                && !is_function_guarded_by_function_exists(context, name)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedFunction,
                    crate::class_casing::undefined_function_message(
                        analyzer,
                        name,
                        context.namespace,
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    } else {
        if let Some(callee_type) = analysis_data.expr_types.get(&callee_pos).cloned() {
            if enforce_mutation_free
                && (callee_type.is_mixed() || union_contains_non_pure_callable(&callee_type))
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureFunctionCall,
                    "Potentially impure callable invocation in mutation-free context",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if callee_type.is_single() {
                if let Some(TAtomic::TLiteralString { value }) = callee_type.get_single() {
                    if value.is_empty() {
                        let (line, col) = analyzer.get_line_column(pos.0);
                        analysis_data.add_issue(Issue::new(
                            IssueKind::InvalidFunctionCall,
                            "Cannot call empty string as a function",
                            analyzer.file_path,
                            pos.0,
                            pos.1,
                            line,
                            col,
                        ));
                    }
                }
            }

            validate_direct_callable_invocation(
                analyzer,
                &callee_type,
                &args,
                &arg_positions,
                analysis_data,
                context,
                pos,
            );

            // A literal-string callee naming a known function gets the same
            // by-ref / @param-out write-backs as a direct named call — Psalm
            // resolves literal callables to the named function, so
            // `$f = "fn"; $f($arr);` widens a by-ref `$arr` identically.
            if let Some(TAtomic::TLiteralString { value }) = callee_type.get_single() {
                let resolved = resolve_function(analyzer, value, false, None, context)
                    .or_else(|| resolve_function(analyzer, value, true, None, context));
                if let Some(function_info) = resolved
                    && function_info.params.iter().any(|param| param.by_ref)
                {
                    let function_id = function_info.name;
                    let params = function_info.params.clone();
                    let functionlike_template_types = function_info.template_types.clone();
                    let empty_templates = TemplateResult::default();
                    apply_param_out_types(
                        analyzer,
                        function_id,
                        &functionlike_template_types,
                        &args,
                        &arg_positions,
                        &params,
                        analysis_data,
                        context,
                        &empty_templates,
                        pos,
                    );
                }
            }

            if let Some(return_type) = callable_validation::infer_callee_return_type(&callee_type) {
                analysis_data.expr_types.insert(pos, Rc::new(return_type));
                return;
            }

            if let Some(return_type) = infer_invokable_object_return_type(
                analyzer,
                &callee_type,
                &args,
                &arg_positions,
                analysis_data,
                context,
            ) {
                analysis_data.expr_types.insert(pos, Rc::new(return_type));
                return;
            }

            // A literal "Class::method" callee resolves like a static call
            // (Psalm builds a MethodIdentifier and calls methodExists, which
            // records the reference); other strings are possibly-callable and
            // produce mixed without complaint.
            if let Some(TAtomic::TLiteralString { value }) = callee_type.get_single()
                && let Some((class_part, method_part)) = value.split_once("::")
            {
                let class_part = class_part.trim_start_matches('\\');
                let class_id = analyzer.interner.intern(class_part);
                if let Some(class_info) = analyzer.codebase.get_class(class_id) {
                    if analyzer.config.find_unused_code
                        && context.self_class != Some(class_id)
                    {
                        analysis_data.referenced_classes.insert(class_id);
                    }
                    if let Some(method_info) = callable_validation::get_method_info(
                        analyzer,
                        class_info,
                        method_part,
                    ) {
                        if analyzer.config.find_unused_code {
                            let method_lc =
                                analyzer.interner.intern(&method_part.to_lowercase());
                            analysis_data
                                .referenced_class_members
                                .insert((class_id, method_lc));
                            if let Some(declaring) = method_info.declaring_class {
                                analysis_data
                                    .referenced_class_members
                                    .insert((declaring, method_lc));
                            }
                            if context.inside_use() {
                                analysis_data
                                    .method_returns_used
                                    .insert((class_id, method_lc));
                                if let Some(declaring) = method_info.declaring_class {
                                    analysis_data
                                        .method_returns_used
                                        .insert((declaring, method_lc));
                                }
                            }
                        }
                        let return_type = method_info
                            .get_return_type()
                            .cloned()
                            .unwrap_or_else(TUnion::mixed);
                        analysis_data.expr_types.insert(pos, Rc::new(return_type));
                        return;
                    }
                }
            }
        } else if enforce_mutation_free {
            let (line, col) = analyzer.get_line_column(pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::ImpureFunctionCall,
                "Potentially impure callable invocation in mutation-free context",
                analyzer.file_path,
                pos.0,
                pos.1,
                line,
                col,
            ));
        }
    }

    // Fall back to mixed
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::mixed()));
}

fn add_function_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    functionlike_id: FunctionLikeIdentifier,
    function_info: Option<&pzoom_code_info::FunctionLikeInfo>,
    pos: Pos,
    arg_positions: &[Pos],
    return_type: TUnion,
) -> TUnion {
    function_call_return_type_fetcher::add_dataflow(
        analyzer,
        &functionlike_id,
        function_info,
        arg_positions,
        pos,
        return_type,
        analysis_data,
    )
}

/// Verify argument types against function parameter types.
pub(crate) fn get_template_defaults(
    func_info: &pzoom_code_info::FunctionLikeInfo,
) -> TemplateResult {
    let mut template_result = TemplateResult::default();

    for template_type in &func_info.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    template_result
}

pub(crate) fn get_class_template_defaults(
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> TemplateResult {
    let mut template_result = TemplateResult::default();

    for template_type in &class_info.template_types {
        crate::template::template_types_insert(
            &mut template_result,
            template_type.name,
            template_type.defining_entity,
            template_type.as_type.clone(),
        );
    }

    template_result
}

pub(crate) use super::class_template_param_collector::infer_class_template_replacements_from_type_params;

/// Folds a class's `@extends`/`@implements` mappings into `template_result`'s
/// lower bounds (`[ancestor template][ancestor] -> mapped type`).
pub(crate) fn infer_class_template_replacements_from_extended_params(
    template_result: &mut TemplateResult,
    class_info: &pzoom_code_info::ClassLikeInfo,
) {
    for (ancestor_class, template_map) in &class_info.template_extended_params {
        for (template_name, replacement) in template_map {
            crate::template::lower_bounds_insert_combined(
                template_result,
                *template_name,
                pzoom_code_info::GenericParent::ClassLike(*ancestor_class),
                replacement.clone(),
            );
        }
    }
}

pub(crate) fn overlay_template_replacements(target: &mut TemplateResult, incoming: TemplateResult) {
    crate::template::lower_bounds_extend_overlay(target, incoming);
}

fn infer_function_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    template_result: &mut TemplateResult,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    infer_template_replacements_from_args(
        analyzer,
        &args,
        arg_positions,
        &func_info.params,
        template_result,
        analysis_data,
        context,
    )
}

pub(crate) fn infer_template_replacements_from_args(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    template_result: &mut TemplateResult,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) {
    if template_result.template_types.is_empty() {
        return;
    }

    // Named-callable args carrying their own fn-level templates are deferred
    // to a second phase: Psalm evaluates the array arg before the callable
    // (array_map's reversed evaluation + HighOrderFunctionArgHandler), so the
    // callable's templates can be solved against bounds the plain args bound.
    let mut deferred_high_order_args: Vec<(
        Pos,
        &pzoom_code_info::functionlike_info::ParamInfo,
        TUnion,
    )> = Vec::new();

    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        let Some(arg_type) =
            crate::expr::call::arguments_analyzer::get_argument_value_type(
                analysis_data,
                arg,
                arg_pos,
            )
        else {
            continue;
        };

        // An unpacked variadic arg binds templates through its ELEMENT type
        // (Psalm's standin replacement over the spread's value type):
        // `array_merge([], ...$arrays)` binds TKey/TValue from each array in
        // $arrays, not from the spread list itself.
        let arg_type = if arg.is_unpacked() {
            match crate::expr::call::arguments_analyzer::unpacked_element_type_for_templates(
                analyzer.codebase,
                &arg_type,
            ) {
                Some(element_type) => std::rc::Rc::new(element_type),
                None => continue,
            }
        } else {
            arg_type
        };

        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };

        let Some(param) = param else {
            continue;
        };
        let Some(param_type) = param.get_type() else {
            continue;
        };

        let arg_type = if callable_validation::union_has_callable(param_type)
            && !callable_validation::union_has_callable(&arg_type)
        {
            let resolved =
                resolve_callable_union_for_template_inference(analyzer, &arg_type, context)
                    .unwrap_or_else(|| (*arg_type).clone());
            if callable_union_has_own_templates(&resolved) {
                deferred_high_order_args.push((arg_pos, param, resolved));
                continue;
            }
            resolved
        } else if callable_validation::union_has_callable(param_type)
            && callable_union_has_own_templates(&arg_type)
            // Inline closures stay in the direct pass (Psalm's
            // getCallableArgInfo covers named/first-class callables only;
            // closure literals go through handleClosureArg).
            && !callable_validation::is_closure_like_argument(arg)
        {
            // A first-class callable (`id(...)`) carrying its OWN fn-level
            // templates: solving them against the expected callable first
            // (Psalm's HighOrderFunctionArgHandler) keeps them from binding
            // the outer call's templates.
            deferred_high_order_args.push((arg_pos, param, (*arg_type).clone()));
            continue;
        } else if callable_validation::union_has_callable(param_type)
            && let Some(raw_callable) =
                high_order_call_arg_raw_callable(analyzer, arg.value(), context)
        {
            // `map($xs, id())`: the call's recorded type already collapsed the
            // unbound templates; the STORAGE return (Closure(T):T) keeps them
            // solvable (Psalm threads the high-order template result into the
            // arg's analysis instead).
            deferred_high_order_args.push((arg_pos, param, raw_callable));
            continue;
        } else {
            (*arg_type).clone()
        };

        // A type-variable-laden argument (`ArrayIterator<`_0, `_1>`) binds
        // templates from its accumulated lower bounds, not from the variable
        // (which template inference would otherwise treat as mixed).
        let arg_type = crate::template::resolve_type_variables_in_union_deep(
            &arg_type,
            &analysis_data.type_variable_bounds,
        );

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            &arg_type,
            template_result,
        );
    }

    for (arg_pos, param, resolved_callable) in deferred_high_order_args {
        let Some(param_type) = param.get_type() else {
            continue;
        };

        // Solve the callable's own templates against the expected callable
        // (Psalm's HighOrderFunctionArgHandler::remapLowerBounds with the
        // bounds collected so far), then bind the callee's templates from the
        // solved view.
        let expected_with_bounds = replace_templates_in_union(param_type, template_result);
        let expected_callables =
            callable_validation::get_expected_callable_atomics(&expected_with_bounds);

        let mut enhanced_atomics = Vec::with_capacity(resolved_callable.types.len());
        for atomic in &resolved_callable.types {
            enhanced_atomics.push(
                callable_validation::enhance_high_order_callable_atomic(
                    analyzer,
                    atomic,
                    &expected_callables,
                )
                .unwrap_or_else(|| atomic.clone()),
            );
        }
        let mut enhanced = resolved_callable.clone();
        enhanced.types = enhanced_atomics;

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            &enhanced,
            template_result,
        );

        let _ = arg_pos;
    }

    // Infer template replacements from omitted optional args that have known defaults.
    // This is important for conditional template behavior with default literal values.
    for param in params.iter().skip(args.len()) {
        let Some(param_type) = param.get_type() else {
            continue;
        };
        let Some(default_type) = &param.default_type else {
            continue;
        };

        crate::template::standin_type_replacer::infer_template_replacements_from_union(
            analyzer,
            param_type,
            default_type,
            template_result,
        );
    }

    // Psalm ArgumentAnalyzer: templates referenced by a provided argument's
    // parameter type that are still unbound after argument processing bind to
    // their upper bound / declared constraint.
    for (idx, _arg) in args.iter().enumerate() {
        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };
        let Some(param_type) = param.and_then(|param| param.get_type()) else {
            continue;
        };
        crate::template::bind_unbound_param_templates_to_constraints(param_type, template_result);
    }
}

/// For `map($xs, id())`-style args: when the arg is a plain function call
/// whose declared return is a callable carrying the callee's own templates,
/// return that raw return type (the recorded expr type has already collapsed
/// the unbound templates). Psalm's HighOrderFunctionArgHandler TYPE_CALLABLE.
pub(crate) fn high_order_call_arg_raw_callable(
    analyzer: &StatementsAnalyzer<'_>,
    arg_expr: &Expression<'_>,
    context: &BlockContext,
) -> Option<TUnion> {
    use mago_syntax::ast::ast::call::Call;
    use mago_syntax::ast::ast::class_like::member::ClassLikeMemberSelector;

    let return_type = match arg_expr.unparenthesized() {
        Expression::Call(Call::Function(function_call)) => {
            let Expression::Identifier(function_name) = &function_call.function else {
                return None;
            };
            let function_info = resolve_function(
                analyzer,
                function_name.value(),
                function_name.is_fully_qualified(),
                Some(function_name.span().start.offset),
                context,
            )?;
            function_info.get_return_type()?.clone()
        }
        Expression::Call(Call::Method(method_call)) => {
            let ClassLikeMemberSelector::Identifier(method_name) = &method_call.method else {
                return None;
            };
            let object_key =
                crate::expression_identifier::get_expression_var_key(method_call.object)?;
            let receiver_type = context.locals.get(object_key.as_str())?;
            let method_info = receiver_type.types.iter().find_map(|atomic| {
                let pzoom_code_info::TAtomic::TNamedObject { name, .. } = atomic else {
                    return None;
                };
                let class_info = analyzer.codebase.get_class(*name)?;
                crate::expr::call::atomic_method_call_analyzer::resolve_named_object_instance_method(
                    analyzer,
                    class_info,
                    None,
                    method_name.value,
                    None,
                )
                .map(|(_, _, method_info)| method_info)
            })?;
            method_info.get_return_type()?.clone()
        }
        Expression::Call(Call::StaticMethod(static_call)) => {
            let ClassLikeMemberSelector::Identifier(method_name) = &static_call.method else {
                return None;
            };
            let class_id = match static_call.class.unparenthesized() {
                Expression::Identifier(class_name) => analyzer
                    .get_resolved_name(class_name.span().start.offset)
                    .unwrap_or_else(|| analyzer.interner.intern(class_name.value())),
                Expression::Self_(_) | Expression::Static(_) => {
                    analyzer.get_declaring_class()?
                }
                _ => return None,
            };
            let class_info = analyzer.codebase.get_class(class_id)?;
            let (_, _, method_info, _) =
                crate::expr::call::existing_atomic_static_call_analyzer::resolve_named_object_static_method(
                    analyzer,
                    class_info,
                    method_name.value,
                )?;
            method_info.get_return_type()?.clone()
        }
        _ => return None,
    };

    (callable_validation::union_has_callable(&return_type)
        && callable_union_has_own_templates(&return_type))
    .then_some(return_type)
}

/// Whether a resolved callable union mentions its *own* function-level
/// template params (e.g. `"from_other"` resolving to
/// `callable(ThingType): ThingType|Bar`).
fn callable_union_has_own_templates(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        callable_validation::get_callable_params(atomic).is_some_and(|params| {
            params.iter().any(|param| {
                union_mentions_fn_template(&param.param_type)
            })
        }) || match atomic {
            TAtomic::TCallable {
                return_type: Some(return_type),
                ..
            }
            | TAtomic::TClosure {
                return_type: Some(return_type),
                ..
            } => union_mentions_fn_template(return_type),
            _ => false,
        }
    })
}

fn union_mentions_fn_template(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| match atomic {
        TAtomic::TTemplateParam {
            defining_entity: pzoom_code_info::GenericParent::FunctionLike(_),
            ..
        } => true,
        TAtomic::TNamedObject {
            type_params: Some(type_params),
            ..
        } => type_params.iter().any(union_mentions_fn_template),
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
        } => union_mentions_fn_template(key_type) || union_mentions_fn_template(value_type),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_mentions_fn_template(value_type)
        }
        _ => false,
    })
}

/// Psalm's `HighOrderFunctionArgHandler::remapLowerBounds` + arg re-analysis:
/// for each argument that is a FUNCTION CALL whose declared return is a
/// callable carrying the callee's own templates, solve those templates
/// against the outer param's expectation (with the bounds inferred so far),
/// seed the inner call's closure arguments with the solved param types, and
/// re-analyze the argument expression. Returns true when anything was
/// re-analyzed (the caller re-infers its templates from the new arg types).
fn reanalyze_high_order_call_args(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    template_result: &TemplateResult,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> bool {
    use mago_span::HasSpan;
    use mago_syntax::ast::ast::call::Call;

    if template_result.template_types.is_empty() {
        return false;
    }

    let mut reanalyzed_any = false;

    for (idx, arg) in args.iter().enumerate() {
        let param = if idx < func_info.params.len() {
            Some(&func_info.params[idx])
        } else {
            func_info.params.last().filter(|p| p.is_variadic)
        };
        let Some(param_type) = param.and_then(|param| param.get_type()) else {
            continue;
        };
        if !callable_validation::union_has_callable(param_type) {
            continue;
        }

        // The arg must be a plain function call whose STORAGE return is a
        // callable with the callee's own fn-level templates.
        let Expression::Call(Call::Function(inner_call)) = arg.value().unparenthesized() else {
            continue;
        };
        let Expression::Identifier(inner_name) = &inner_call.function else {
            continue;
        };
        let Some(inner_info) = resolve_function(
            analyzer,
            inner_name.value(),
            inner_name.is_fully_qualified(),
            Some(inner_name.span().start.offset),
            context,
        ) else {
            continue;
        };
        let Some(raw_return) = inner_info.get_return_type() else {
            continue;
        };
        if !callable_validation::union_has_callable(raw_return)
            || !callable_union_has_own_templates(raw_return)
        {
            continue;
        }

        // Nested closure arguments to refine.
        let closure_args: Vec<(usize, u32)> = inner_call
            .argument_list
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(inner_idx, inner_arg)| {
                callable_validation::get_closure_like_argument_offset(inner_arg)
                    .map(|offset| (inner_idx, offset))
            })
            .collect();
        if closure_args.is_empty() {
            continue;
        }

        // Solve the inner callee's templates: each callable in its declared
        // return matches param-for-param against the outer expectation (with
        // the outer bounds substituted) — Psalm's remapLowerBounds.
        let expected_with_bounds = replace_templates_in_union(param_type, template_result);
        let expected_callables =
            callable_validation::get_expected_callable_atomics(&expected_with_bounds);
        let mut inner_result = get_template_defaults(&inner_info);
        if inner_result.template_types.is_empty() {
            continue;
        }
        let mut solved_any = false;
        for raw_atomic in &raw_return.types {
            let (TAtomic::TClosure {
                params: Some(raw_params),
                ..
            }
            | TAtomic::TCallable {
                params: Some(raw_params),
                ..
            }) = raw_atomic
            else {
                continue;
            };
            for expected_atomic in &expected_callables {
                let (TAtomic::TClosure {
                    params: Some(expected_params),
                    ..
                }
                | TAtomic::TCallable {
                    params: Some(expected_params),
                    ..
                }) = expected_atomic
                else {
                    continue;
                };
                for (raw_param, expected_param) in raw_params.iter().zip(expected_params.iter()) {
                    crate::template::standin_type_replacer::infer_template_replacements_from_union(
                        analyzer,
                        &raw_param.param_type,
                        &expected_param.param_type,
                        &mut inner_result,
                    );
                    solved_any = true;
                }
            }
        }
        if !solved_any || inner_result.lower_bounds.is_empty() {
            continue;
        }

        // Seed the inner closures with the solved param types and re-analyze
        // the whole argument expression; first retract the issues its
        // unrefined first pass emitted.
        let mut seeded_offsets = Vec::new();
        for (inner_idx, closure_offset) in &closure_args {
            let Some(inner_param_type) = inner_info
                .params
                .get(*inner_idx)
                .and_then(|inner_param| inner_param.get_type())
            else {
                continue;
            };
            if !callable_validation::union_has_callable(inner_param_type) {
                continue;
            }
            let seeded = replace_templates_in_union(inner_param_type, &inner_result);
            context
                .expected_callable_arg_types
                .insert(*closure_offset, seeded);
            seeded_offsets.push(*closure_offset);
        }
        if seeded_offsets.is_empty() {
            continue;
        }

        let arg_span = arg.value().span();
        analysis_data.issues.retain(|issue| {
            issue.location.start_offset < arg_span.start.offset
                || issue.location.start_offset > arg_span.end.offset
        });
        crate::expression_analyzer::analyze(analyzer, arg.value(), analysis_data, context);
        for closure_offset in seeded_offsets {
            context.expected_callable_arg_types.remove(&closure_offset);
        }
        reanalyzed_any = true;
    }

    reanalyzed_any
}

pub(crate) fn replace_templates_in_union(union: &TUnion, template_result: &TemplateResult) -> TUnion {
    replace_templates_in_union_in(None, union, template_result)
}


/// [`replace_templates_in_union`] with a codebase, which lets conditional
/// types pick a branch (Psalm's TemplateInferredTypeReplacer needs the
/// codebase for containment checks); without one they fall back to the union
/// of their branches.
pub(crate) fn replace_templates_in_union_in(
    codebase: Option<&pzoom_code_info::CodebaseInfo>,
    union: &TUnion,
    template_result: &TemplateResult,
) -> TUnion {
    let standin_replaced = crate::template::standin_type_replacer::substitute_templates_in_union(
        union,
        template_result,
    );

    template::inferred_type_replacer::replace_in(codebase, &standin_replaced, template_result)
}

#[derive(Clone)]
pub(crate) struct ArrayLikeInfo {
    pub(crate) key_type: TUnion,
    pub(crate) value_type: TUnion,
    pub(crate) is_list: bool,
    pub(crate) is_non_empty: bool,
}

pub(crate) fn get_builtin_type_check_atomic(function_name: &str) -> Option<TAtomic> {
    let normalized_name = function_name.to_ascii_lowercase();

    Some(match normalized_name.as_str() {
        "is_string" => TAtomic::TString,
        "is_int" | "is_integer" | "is_long" => TAtomic::TInt,
        "is_float" | "is_double" | "is_real" => TAtomic::TFloat,
        "is_bool" => TAtomic::TBool,
        "is_array" => TAtomic::TArray {
            key_type: Box::new(TUnion::array_key()),
            value_type: Box::new(TUnion::mixed()),
        },
        "is_object" => TAtomic::TObject,
        "is_null" => TAtomic::TNull,
        "is_numeric" => TAtomic::TNumeric,
        "is_resource" => TAtomic::TResource,
        "is_scalar" => TAtomic::TScalar,
        "is_iterable" => TAtomic::TIterable {
            key_type: Box::new(TUnion::mixed()),
            value_type: Box::new(TUnion::mixed()),
        },
        _ => return None,
    })
}

pub(crate) fn infer_builtin_type_check_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    asserted_atomic: TAtomic,
    arg_has_var_key: bool,
) -> Option<TUnion> {
    // A reconcilable-lvalue argument is reconciled (and refined) through the
    // assertion machinery, which emits any redundancy/impossibility itself —
    // narrowing the return type here would clobber that refinement. Only decide the
    // result for arguments with no reconcilable key.
    if arg_has_var_key {
        return Some(TUnion::bool());
    }

    let Some(arg_type) = arg_positions
        .first()
        .and_then(|pos| analysis_data.expr_types.get(&*pos).cloned())
        .map(|t| (*t).clone())
    else {
        return Some(TUnion::bool());
    };

    // Only decide when the argument type is concrete: mixed/never and template
    // parameters keep the result a plain `bool`.
    if arg_type.is_mixed()
        || arg_type.is_nothing()
        || arg_type.from_calculation
        || arg_type
            .types
            .iter()
            .any(|atomic| matches!(atomic, TAtomic::TTemplateParam { .. }))
    {
        return Some(TUnion::bool());
    }

    let asserted_union = TUnion::new(asserted_atomic.clone());
    let mut comparison_result =
        crate::type_comparator::type_comparison_result::TypeComparisonResult::new();
    let always_matches = crate::type_comparator::union_type_comparator::is_contained_by(
        analyzer.codebase,
        &arg_type,
        &asserted_union,
        false,
        false,
        &mut comparison_result,
    );

    let mut result = if always_matches {
        // Every member of the argument type already satisfies the check — always true.
        TUnion::new(TAtomic::TTrue)
    } else if assertion_reconciler::intersect_union_with_atomic(&arg_type, &asserted_atomic, analyzer)
        .is_none()
    {
        // No member can be of the asserted type — always false.
        TUnion::new(TAtomic::TFalse)
    } else {
        // Otherwise the runtime check genuinely discriminates.
        return Some(TUnion::bool());
    };
    result.from_docblock = arg_type.from_docblock;
    Some(result)
}

pub(crate) fn infer_string_transform_return_type(subject_type: &TUnion) -> Option<TUnion> {
    if subject_type.is_mixed() {
        return Some(TUnion::mixed());
    }

    let mut result_types = Vec::new();

    if union_contains_stringish(subject_type) {
        result_types.push(TAtomic::TString);
    }

    if let Some(array_info) = extract_array_like_info_from_union(subject_type) {
        let key_type = Box::new(normalize_array_key_union(&array_info.key_type));
        let value_type = Box::new(TUnion::string());
        let array_atomic = if array_info.is_list {
            if array_info.is_non_empty {
                TAtomic::TNonEmptyList { value_type }
            } else {
                TAtomic::TList { value_type }
            }
        } else if array_info.is_non_empty {
            TAtomic::TNonEmptyArray {
                key_type,
                value_type,
            }
        } else {
            TAtomic::TArray {
                key_type,
                value_type,
            }
        };

        if !result_types.contains(&array_atomic) {
            result_types.push(array_atomic);
        }
    }

    if result_types.is_empty() {
        None
    } else {
        Some(TUnion::from_types(result_types))
    }
}

pub(crate) fn is_default_array_filter_callback(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> bool {
    if args.len() < 2 || arg_positions.len() < 2 {
        return true;
    }

    analysis_data
        .expr_types.get(&arg_positions[1]).cloned()
        .is_some_and(|callback_type| callback_type.is_null())
}

pub(crate) fn infer_array_filter_return_atomic(
    atomic: &TAtomic,
    callback_is_default: bool,
) -> Option<TAtomic> {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        } => Some(TAtomic::TArray {
            key_type: key_type.clone(),
            value_type: Box::new(if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            }),
        }),
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => {
            let filtered_value = if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            };

            // A custom callback can reject every element, so non-emptiness
            // never survives (Psalm's ArrayFilterReturnTypeProvider always
            // returns a plain array). The default callback keeps it only
            // when every value is provably truthy.
            if !callback_is_default || !(**value_type).is_always_truthy() {
                Some(TAtomic::TArray {
                    key_type: key_type.clone(),
                    value_type: Box::new(filtered_value),
                })
            } else {
                Some(TAtomic::TNonEmptyArray {
                    key_type: key_type.clone(),
                    value_type: Box::new(filtered_value),
                })
            }
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            let filtered_value = if callback_is_default {
                narrow_union_to_truthy(value_type)
            } else {
                (**value_type).clone()
            };

            Some(TAtomic::TArray {
                key_type: Box::new(TUnion::int()),
                value_type: Box::new(filtered_value),
            })
        }
        TAtomic::TKeyedArray {
            properties,
            sealed,
            fallback_key_type,
            fallback_value_type,
            is_list,
        } => {
            let had_one = properties.len() == 1;
            // Psalm's ArrayFilterReturnTypeProvider keeps per-property
            // precision only for the DEFAULT callback (the provider's keyed
            // path is gated on `!isset($call_args[1])`); a custom callback
            // degrades the shape to a generic possibly-empty array of the
            // keys/values, which the truthy/empty reconcilers can then
            // narrow to non-empty.
            if !callback_is_default {
                let mut key_union: Option<TUnion> = None;
                let mut value_union: Option<TUnion> = None;
                let mut add_key = |atomic: TAtomic| {
                    let next = TUnion::new(atomic);
                    key_union = Some(match key_union.take() {
                        None => next,
                        Some(existing) => {
                            pzoom_code_info::combine_union_types(&existing, &next, false)
                        }
                    });
                };
                for (key, property_type) in properties.iter() {
                    match key {
                        pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                            add_key(TAtomic::TLiteralInt { value: *value })
                        }
                        pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                            add_key(TAtomic::TLiteralString {
                                value: value.clone(),
                            })
                        }
                    }
                    value_union = Some(match value_union.take() {
                        None => property_type.clone(),
                        Some(existing) => pzoom_code_info::combine_union_types(
                            &existing,
                            property_type,
                            false,
                        ),
                    });
                }
                if let Some(fallback) = fallback_value_type {
                    value_union = Some(match value_union.take() {
                        None => (**fallback).clone(),
                        Some(existing) => {
                            pzoom_code_info::combine_union_types(&existing, fallback, false)
                        }
                    });
                }
                if let Some(fallback_key) = fallback_key_type {
                    let key = key_union.take();
                    key_union = Some(match key {
                        None => (**fallback_key).clone(),
                        Some(existing) => {
                            pzoom_code_info::combine_union_types(&existing, fallback_key, false)
                        }
                    });
                }
                let mut value_union = value_union.unwrap_or_else(TUnion::nothing);
                value_union.possibly_undefined = false;
                return Some(TAtomic::TArray {
                    key_type: Box::new(key_union.unwrap_or_else(TUnion::array_key)),
                    value_type: Box::new(value_union),
                });
            }

            let mut next_properties = rustc_hash::FxHashMap::default();

            for (key, property_type) in properties.iter() {
                if property_type.is_always_falsy() {
                    continue;
                }

                let narrowed_type = narrow_union_to_truthy(property_type);

                if property_type.is_always_truthy() {
                    next_properties.insert(key.clone(), narrowed_type);
                } else {
                    next_properties.insert(
                        key.clone(),
                        mark_union_as_possibly_undefined(&narrowed_type),
                    );
                }
            }

            let next_fallback_value = fallback_value_type.as_ref().map(|fallback| {
                Box::new(if callback_is_default {
                    narrow_union_to_truthy(fallback)
                } else {
                    (**fallback).clone()
                })
            });

            // Mirror Psalm's ArrayFilterReturnTypeProvider: list-ness is only
            // preserved when the original keyed array had exactly one property
            // (so removing it can't leave a gap in the integer key sequence).
            let next_is_list = *is_list && had_one;

            Some(TAtomic::TKeyedArray {
                properties: std::sync::Arc::new(next_properties),
                is_list: next_is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.clone(),
                fallback_value_type: next_fallback_value,
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            let mut inferred = Vec::new();
            for nested_atomic in &as_type.types {
                let Some(filtered_atomic) =
                    infer_array_filter_return_atomic(nested_atomic, callback_is_default)
                else {
                    continue;
                };

                if !inferred.contains(&filtered_atomic) {
                    inferred.push(filtered_atomic);
                }
            }

            if inferred.is_empty() {
                None
            } else {
                Some(TAtomic::TArray {
                    key_type: Box::new(TUnion::array_key()),
                    value_type: Box::new(TUnion::from_types(inferred)),
                })
            }
        }
        _ => None,
    }
}

fn mark_union_as_possibly_undefined(union: &TUnion) -> TUnion {
    if union.possibly_undefined {
        return union.clone();
    }

    let mut possibly_undefined = union.clone();
    possibly_undefined.possibly_undefined = true;
    possibly_undefined
}

pub(crate) fn extract_array_like_info_from_union(union: &TUnion) -> Option<ArrayLikeInfo> {
    let mut combined: Option<ArrayLikeInfo> = None;

    for atomic in &union.types {
        let Some(info) = extract_array_like_info_from_atomic(atomic) else {
            continue;
        };

        combined = Some(if let Some(existing) = combined {
            combine_array_like_info(existing, info)
        } else {
            info
        });
    }

    combined
}

fn union_contains_stringish(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TClassString { .. }
                | TAtomic::TLiteralClassString { .. }
        )
    })
}

fn extract_array_like_info_from_atomic(atomic: &TAtomic) -> Option<ArrayLikeInfo> {
    match atomic {
        TAtomic::TArray {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: false,
        }),
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: true,
        }),
        TAtomic::TList { value_type } => Some(ArrayLikeInfo {
            key_type: TUnion::int(),
            value_type: (**value_type).clone(),
            is_list: true,
            is_non_empty: false,
        }),
        TAtomic::TNonEmptyList { value_type } => Some(ArrayLikeInfo {
            key_type: TUnion::int(),
            value_type: (**value_type).clone(),
            is_list: true,
            is_non_empty: true,
        }),
        TAtomic::TKeyedArray {
            properties,
            is_list,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            let key_type = extract_keyed_array_key_type(properties, fallback_key_type.as_deref());
            let value_type = crate::template::standin_type_replacer::extract_keyed_array_value_type(
                properties,
                fallback_value_type.as_deref(),
            );

            Some(ArrayLikeInfo {
                key_type,
                value_type,
                is_list: *is_list,
                is_non_empty: !properties.is_empty(),
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => extract_array_like_info_from_union(as_type),
        _ => None,
    }
}

pub(crate) fn extract_iterable_like_info_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    union: &TUnion,
) -> Option<ArrayLikeInfo> {
    let mut combined: Option<ArrayLikeInfo> = None;

    for atomic in &union.types {
        let Some(info) = extract_iterable_like_info_from_atomic(analyzer, atomic) else {
            continue;
        };

        combined = Some(if let Some(existing) = combined {
            combine_array_like_info(existing, info)
        } else {
            info
        });
    }

    combined
}

fn extract_iterable_like_info_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    atomic: &TAtomic,
) -> Option<ArrayLikeInfo> {
    if let Some(array_info) = extract_array_like_info_from_atomic(atomic) {
        return Some(array_info);
    }

    match atomic {
        TAtomic::TIterable {
            key_type,
            value_type,
        } => Some(ArrayLikeInfo {
            key_type: (**key_type).clone(),
            value_type: (**value_type).clone(),
            is_list: false,
            is_non_empty: false,
        }),
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
            if !named_object_is_traversable(analyzer, *name) {
                return None;
            }

            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some(ArrayLikeInfo {
                key_type,
                value_type,
                is_list: false,
                is_non_empty: false,
            })
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            extract_iterable_like_info_from_union(analyzer, as_type)
        }
        TAtomic::TObjectIntersection { types } => {
            let mut combined: Option<ArrayLikeInfo> = None;
            for intersection_atomic in types {
                let Some(info) =
                    extract_iterable_like_info_from_atomic(analyzer, intersection_atomic)
                else {
                    continue;
                };

                combined = Some(if let Some(existing) = combined {
                    combine_array_like_info(existing, info)
                } else {
                    info
                });
            }

            combined
        }
        _ => None,
    }
}

fn combine_array_like_info(left: ArrayLikeInfo, right: ArrayLikeInfo) -> ArrayLikeInfo {
    ArrayLikeInfo {
        key_type: combine_union_types(&left.key_type, &right.key_type, false),
        value_type: combine_union_types(&left.value_type, &right.value_type, false),
        is_list: left.is_list && right.is_list,
        is_non_empty: left.is_non_empty && right.is_non_empty,
    }
}

fn extract_keyed_array_key_type(
    properties: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, TUnion>,
    fallback_key_type: Option<&TUnion>,
) -> TUnion {
    let mut key_type = fallback_key_type.cloned().unwrap_or_else(TUnion::nothing);

    for key in properties.keys() {
        let literal_key_type = match key {
            pzoom_code_info::ArrayKey::Int(value) => {
                TUnion::new(TAtomic::TLiteralInt { value: *value })
            }
            pzoom_code_info::ArrayKey::String(value) => TUnion::new(TAtomic::TLiteralString {
                value: value.clone(),
            }),
        };

        key_type = if key_type.is_nothing() {
            literal_key_type
        } else {
            combine_union_types(&key_type, &literal_key_type, false)
        };
    }

    if key_type.is_nothing() {
        TUnion::array_key()
    } else {
        key_type
    }
}

pub(crate) fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
    if name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
    {
        return true;
    }

    analyzer.codebase.get_class(name).is_some_and(|class_info| {
        class_info.interfaces.contains(&StrId::TRAVERSABLE)
            || class_info
                .all_parent_interfaces
                .iter()
                .any(|interface| *interface == StrId::TRAVERSABLE)
    })
}

/// Port of Psalm's `Reconciler::refineArrayKey`: array-key-compatible atomics
/// pass through (string/int families and `array-key` itself), a template
/// param survives with its bound refined recursively, and anything else
/// (e.g. `mixed`) degrades to `array-key`.
pub(crate) fn normalize_array_key_union(key_type: &TUnion) -> TUnion {
    if key_type.is_nothing() {
        return TUnion::nothing();
    }

    let mut refined_types = Vec::with_capacity(key_type.types.len());
    for atomic in &key_type.types {
        let refined_atomic = match atomic {
            TAtomic::TTemplateParam {
                name,
                defining_entity,
                as_type,
            } => TAtomic::TTemplateParam {
                name: *name,
                defining_entity: *defining_entity,
                as_type: Box::new(normalize_array_key_union(as_type)),
            },
            atomic if atomic_is_array_key_compatible(atomic) => atomic.clone(),
            _ => TAtomic::TArrayKey,
        };
        if !refined_types.contains(&refined_atomic) {
            refined_types.push(refined_atomic);
        }
    }

    let mut refined = TUnion::from_types(refined_types);
    refined.from_docblock = key_type.from_docblock;
    refined
}

/// Psalm's refineArrayKey pass-through set: `array-key` plus the TString and
/// TInt families (including literals).
fn atomic_is_array_key_compatible(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TArrayKey
            | TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TInt
            | TAtomic::TLiteralInt { .. }
            | TAtomic::TIntRange { .. }
            | TAtomic::TDependentGetClass { .. }
            | TAtomic::TDependentGetType { .. }
    )
}

pub(crate) fn get_literal_bool_from_union(union: &TUnion) -> Option<bool> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn is_forbidden_function_call(
    analyzer: &StatementsAnalyzer<'_>,
    called_name: &str,
    resolved_function: Option<&pzoom_code_info::FunctionLikeInfo>,
) -> bool {
    if analyzer.config.forbidden_functions.is_empty() {
        return false;
    }

    if let Some(function_info) = resolved_function {
        let resolved_name = analyzer.interner.lookup(function_info.name);
        return is_forbidden_function_name(analyzer, resolved_name.as_ref());
    }

    let normalized = called_name.strip_prefix('\\').unwrap_or(called_name);
    if normalized.contains('\\') {
        return false;
    }

    is_forbidden_function_name(analyzer, normalized)
}

fn is_forbidden_function_name(analyzer: &StatementsAnalyzer<'_>, function_name: &str) -> bool {
    let normalized = function_name.strip_prefix('\\').unwrap_or(function_name);
    analyzer.config.forbidden_functions.iter().any(|forbidden| {
        forbidden
            .strip_prefix('\\')
            .unwrap_or(forbidden.as_str())
            .eq_ignore_ascii_case(normalized)
    })
}

fn apply_function_defined_constants_side_effects(
    function_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
) {
    for (const_id, const_type) in &function_info.defined_constants {
        context
            .defined_constants
            .insert(*const_id, const_type.clone());
    }
}

/// Resolve a function by name, considering namespace context.
///
/// PHP function resolution:
/// 1. If fully qualified (starts with \), use it directly
/// 2. If unqualified, first try current_namespace\function_name
/// 3. Fall back to global namespace
pub(crate) fn resolve_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    is_fully_qualified: bool,
    name_offset: Option<u32>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    // Function names resolve case-sensitively (stricter than PHP); a leading
    // backslash is the only normalization applied.
    if let Some(offset) = name_offset
        && let Some(resolved_name) = analyzer.get_resolved_name(offset)
    {
        if let Some(func_info) = analyzer.codebase.get_function(resolved_name) {
            return Some(func_info);
        }

        let resolved_name = analyzer.interner.lookup(resolved_name);
        let trimmed = resolved_name.trim_start_matches('\\');
        if trimmed.len() != resolved_name.len() {
            let func_id = analyzer.interner.intern(trimmed);
            if let Some(func_info) = analyzer.codebase.get_function(func_id) {
                return Some(func_info);
            }
        }
    }

    if is_fully_qualified {
        // Strip leading backslash and look up directly
        let clean_name = name.strip_prefix('\\').unwrap_or(name);
        let func_id = analyzer.interner.intern(clean_name);
        return analyzer.codebase.get_function(func_id);
    }

    // Try namespace-qualified lookup first
    if let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, name);
        let func_id = analyzer.interner.intern(&qualified_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }
    }

    // Fall back to global namespace
    let func_id = analyzer.interner.intern(name);
    analyzer.codebase.get_function(func_id)
}

/// Extract the function name from a function call expression.
/// Returns (name, is_fully_qualified, identifier_offset).
fn get_function_name<'a>(expr: &'a Expression<'a>) -> (Option<&'a str>, bool, Option<u32>) {
    match expr.unparenthesized() {
        Expression::Identifier(id) => (
            Some(id.value()),
            id.is_fully_qualified(),
            Some(id.span().start.offset),
        ),
        _ => (None, false, None),
    }
}

fn function_call_is_mutation_free(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> bool {
    if function_is_mutation_free(function_info) {
        return true;
    }

    // `assert()` has no side effects of its own (the asserted expression is analyzed
    // separately), so it is mutation-free. Psalm treats it as pure.
    if function_info.name == StrId::ASSERT {
        return true;
    }


    if is_array_map_function_name(function_info.name) {
        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 0);
    }

    if is_array_filter_function_name(function_info.name) {
        if args.len() < 2 {
            return true;
        }

        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 1);
    }

    // The user-comparator sort functions only mutate their by-reference array
    // argument (a local), so they are mutation-free when the comparator is pure -
    // matching Psalm.
    if matches!(
        function_info.name,
        StrId::USORT | StrId::UASORT | StrId::UKSORT
    ) {
        if args.len() < 2 {
            return true;
        }
        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 1);
    }

    // Psalm's in_call_map arm: a stub builtin's purity comes from
    // Functions::isCallMapFunctionPure — anything not in the impure list
    // (with params, a non-void return and actual args) defaults to PURE;
    // by-ref builtins like reset()/array_pop() only mutate locals.
    let is_builtin = function_info.in_call_map
        || analyzer
            .codebase
            .files
            .get(&function_info.file_path)
            .is_some_and(|file_info| file_info.is_stub);
    if is_builtin {
        let name = analyzer.interner.lookup(function_info.name);
        let mut must_use = true;
        return is_call_map_function_pure(
            analyzer,
            function_info,
            &name,
            args,
            arg_positions,
            analysis_data,
            &mut must_use,
        );
    }

    false
}

pub(crate) fn callback_argument_is_pure(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
    callback_index: usize,
) -> bool {
    let Some(_callback_arg) = args.get(callback_index) else {
        return true;
    };
    let Some(callback_pos) = arg_positions.get(callback_index).copied() else {
        return true;
    };
    let Some(callback_type) = analysis_data.expr_types.get(&callback_pos).cloned() else {
        return false;
    };

    let resolved = resolve_callable_union_for_template_inference(analyzer, &callback_type, context)
        .unwrap_or_else(|| (*callback_type).clone());

    let mut saw_non_null_candidate = false;
    let mut saw_callable_candidate = false;

    for atomic in &resolved.types {
        match atomic {
            TAtomic::TNull => {}
            TAtomic::TCallable { is_pure, .. } | TAtomic::TClosure { is_pure, .. } => {
                saw_non_null_candidate = true;
                saw_callable_candidate = true;
                if !matches!(is_pure, Some(true)) {
                    return false;
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                saw_non_null_candidate = true;
                if !callable_union_is_pure(as_type) {
                    return false;
                }
                saw_callable_candidate = true;
            }
            _ => {
                return false;
            }
        }
    }

    !saw_non_null_candidate || saw_callable_candidate
}

fn function_is_mutation_free(function_info: &pzoom_code_info::FunctionLikeInfo) -> bool {
    function_info.is_pure || function_info.is_mutation_free
}

fn is_mutation_free_context(analyzer: &StatementsAnalyzer<'_>) -> bool {
    // One definition of "enforcing purity context" — see
    // method_call_analyzer::is_mutation_free_context for Psalm's semantics
    // (a method's own @mutation-free is NOT body-enforced).
    crate::expr::call::method_call_analyzer::is_mutation_free_context(analyzer)
}

fn record_named_function_callsite_argument_types(
    function_id: StrId,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    for (index, arg_pos) in arg_positions.iter().enumerate() {
        let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned() else {
            continue;
        };

        analysis_data.record_function_argument_callsite_type(
            function_id,
            index,
            (*arg_type).clone(),
        );
    }
}

/// Check if a name is a PHP language construct (not a real function).
///
/// These are special syntax that look like function calls but are actually
/// language constructs handled by the parser/compiler. They won't be in stubs.
pub(crate) fn is_language_construct(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        // Output constructs
        "echo"
            | "print"
            // Program termination
            | "die"
            | "exit"
            // Variable inspection (actually functions, but often not in stubs)
            | "isset"
            | "unset"
            | "empty"
            // Include/require (handled separately but can appear as function-like)
            | "include"
            | "include_once"
            | "require"
            | "require_once"
            // Evaluation
            | "eval"
            // List assignment
            | "list"
            // Array literal (not really a function)
            | "array"
    )
}

pub(crate) fn is_array_map_function_name(function_id: StrId) -> bool {
    function_id == StrId::ARRAY_MAP
}

pub(crate) fn is_function_guarded_by_function_exists(
    context: &BlockContext,
    function_name: &str,
) -> bool {
    let key = function_exists_assertion_key(function_name);
    let key_id = VarName::new(&key);

    context
        .locals
        .get(&key_id)
        .is_some_and(|guard_type| !guard_type.is_nothing() && !guard_type.is_always_falsy())
}

fn function_exists_assertion_key(function_name: &str) -> String {
    format!(
        "@function_exists({})",
        function_name.trim_start_matches('\\').to_ascii_lowercase()
    )
}

/// Port of Psalm's `FunctionCallAnalyzer::checkFunctionCallPurity` unused-call
/// branch: a pure function whose return value is discarded reports
/// `UnusedFunctionCall` when unused-variable detection is on.
#[allow(clippy::too_many_arguments)]
fn check_unused_function_call(
    analyzer: &StatementsAnalyzer<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    name: &str,
    func_call: &FunctionCall<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    pos: Pos,
) {
    if !analyzer.config.report_unused
        || context.inside_conditional
        || context.inside_unset
        || context.inside_use()
    {
        return;
    }

    let is_builtin = analyzer
        .codebase
        .files
        .get(&func_info.file_path)
        .is_some_and(|file_info| file_info.is_stub);

    let mut must_use = true;
    let is_pure = if is_builtin {
        // Psalm's in_call_map path: Functions::isCallMapFunctionPure.
        is_call_map_function_pure(
            analyzer,
            func_info,
            name,
            args,
            arg_positions,
            analysis_data,
            &mut must_use,
        )
    } else {
        // User-defined: declared @pure with no assertions.
        func_info.is_pure && func_info.assertions.is_empty()
    };

    if !is_pure || !must_use {
        return;
    }

    // A by-reference argument actually passed means the call has an effect
    // (Psalm's callUsesByReferenceArguments — positional and named).
    if call_uses_by_reference_arguments(analyzer, func_info, args) {
        return;
    }

    // Pure no-return functions may be called for their exception/exit effect.
    if func_info
        .get_return_type()
        .is_some_and(|return_type| return_type.is_nothing())
    {
        return;
    }

    let name_span = func_call.function.span();
    let (line, col) = analyzer.get_line_column(name_span.start.offset);
    analysis_data.add_issue(Issue::new(
        IssueKind::UnusedFunctionCall,
        format!("The call to {} is not used", name.to_ascii_lowercase()),
        analyzer.file_path,
        name_span.start.offset,
        name_span.end.offset,
        line,
        col,
    ));
    let _ = pos;
}

/// Port of Psalm `Functions::isCallMapFunctionPure` (stub-defined builtins).
#[allow(clippy::too_many_arguments)]
fn is_call_map_function_pure(
    analyzer: &StatementsAnalyzer<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    must_use: &mut bool,
) -> bool {
    let function_id = name.trim_start_matches('\\').to_ascii_lowercase();

    if super::impure_functions_list::is_impure_function(&function_id) {
        return false;
    }

    if function_id == "serialize"
        && let Some(arg_pos) = arg_positions.first()
        && let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned()
        && union_can_contain_object(&arg_type)
    {
        return false;
    }

    if function_id.starts_with("image") || function_id.starts_with("readline") {
        return false;
    }

    if (function_id == "var_export" || function_id == "print_r") && args.len() < 2 {
        return false;
    }

    if function_id == "assert" {
        *must_use = false;
        return true;
    }

    if function_id == "func_num_args" || function_id == "func_get_args" {
        return true;
    }

    if (function_id == "count" || function_id == "sizeof")
        && let Some(arg_pos) = arg_positions.first()
        && let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned()
    {
        for atomic in &arg_type.types {
            if let TAtomic::TNamedObject { name: class_id, .. } = atomic
                && let Some(class_info) = analyzer.codebase.get_class(*class_id)
                && let Some(method_info) =
                    callable_validation::get_method_info(analyzer, class_info, "count")
            {
                return method_info.is_mutation_free;
            }
        }
    }

    // Psalm: unknown params in the callmap, a zero-argument call, or a void
    // return type all mean "not reportable as pure".
    if args.is_empty() {
        return false;
    }
    if func_info
        .get_return_type()
        .is_none_or(|return_type| return_type.is_void())
    {
        return false;
    }

    *must_use = function_id != "array_map"
        || args
            .first()
            .is_some_and(|arg| !matches!(arg.value().unparenthesized(), Expression::Closure(_)));

    for (index, param) in func_info.params.iter().enumerate() {
        if let Some(param_type) = &param.param_type
            && callable_validation::union_has_callable(param_type)
            && let Some(arg_pos) = arg_positions.get(index)
            && let Some(arg_type) = analysis_data.expr_types.get(&*arg_pos).cloned()
            && !callable_union_is_pure(&arg_type)
        {
            return false;
        }

        if param.by_ref && args.get(index).is_some() {
            *must_use = false;
        }
    }

    true
}

/// Psalm's `callUsesByReferenceArguments`: whether any actually-passed
/// argument (positional or named) lands on a by-ref parameter.
fn call_uses_by_reference_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
) -> bool {
    if !func_info.params.iter().any(|param| param.by_ref) {
        return false;
    }

    for (index, arg) in args.iter().enumerate() {
        let param = match arg {
            Argument::Named(named_arg) => func_info.params.iter().find(|param| {
                super::arguments_analyzer::matches_named_argument(
                    analyzer,
                    param,
                    named_arg.name.value,
                )
            }),
            Argument::Positional(_) => func_info.params.get(index),
        };
        if param.is_some_and(|param| param.by_ref) {
            return true;
        }
    }

    false
}

/// Approximation of Psalm's `Union::canContainObjectType` for the serialize check.
fn union_can_contain_object(union: &TUnion) -> bool {
    union.types.iter().any(|atomic| match atomic {
        TAtomic::TNamedObject { .. }
        | TAtomic::TObject
        | TAtomic::TObjectWithProperties { .. }
        | TAtomic::TMixed
        | TAtomic::TNonEmptyMixed
        | TAtomic::TMixedFromLoopIsset
        | TAtomic::TTemplateParam { .. }
        // An iterable may be a Traversable object (and may yield objects);
        // closures/callables are or may be objects.
        | TAtomic::TIterable { .. }
        | TAtomic::TClosure { .. }
        | TAtomic::TCallable { .. } => true,
        TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
            union_can_contain_object(value_type)
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            union_can_contain_object(value_type)
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_value_type,
            ..
        } => {
            properties.values().any(union_can_contain_object)
                || fallback_value_type
                    .as_ref()
                    .is_some_and(|fallback| union_can_contain_object(fallback))
        }
        _ => false,
    })
}
