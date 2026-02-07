//! Function call analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::Argument;
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;
use std::collections::BTreeMap;

use pzoom_code_info::algebra::{get_truths_from_formula, simplify_cnf};
use pzoom_code_info::functionlike_info::{AssertionType, ConditionalReturnCondition, ParamInfo};
use pzoom_code_info::{
    Assertion, DataFlowNode, FunctionLikeIdentifier, FunctionLikeParameter, Issue, IssueKind,
    TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::data_flow::{add_default_dataflow_paths, make_data_flow_node_position};
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::internal_access::{can_access_internal, format_internal_scope_phrase};
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

use super::{
    argument_analyzer, arguments_analyzer, callable_validation, function_call_return_type_fetcher,
    named_function_call_handler,
};

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
    let callee_pos =
        expression_analyzer::analyze(analyzer, func_call.function, analysis_data, context);

    // Try to get the function name and whether it's fully qualified.
    // We need this before analyzing arguments to predeclare by-ref out variables.
    let (func_name, is_fq, name_offset) = get_function_name(func_call.function);
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
            let span = arg.span();
            (span.start.offset, span.end.offset)
        })
        .collect();

    if let Some(func_info) = pre_resolved_func_info {
        analyze_arguments_with_callable_context(
            analyzer,
            &args,
            &arg_positions,
            &func_info.params,
            &get_template_defaults(func_info),
            analysis_data,
            context,
        );
    } else {
        for arg in &args {
            argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        }
    }

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
                pos,
                &arg_positions,
                return_type,
            );
            analysis_data.set_expr_type(pos, return_type);
            return;
        }

        // Resolve the function name considering namespace context
        let func_info = pre_resolved_func_info;
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
            analysis_data.set_expr_type(pos, TUnion::mixed());
            return;
        }

        if let Some(func_info) = func_info {
            let is_class_alias_call = name.eq_ignore_ascii_case("class_alias");
            apply_function_defined_constants_side_effects(func_info, context);
            let template_defaults = get_template_defaults(func_info);
            let template_replacements = infer_function_template_replacements(
                analyzer,
                func_call,
                &arg_positions,
                func_info,
                &template_defaults,
                analysis_data,
                context,
            );

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
            if !is_class_alias_call {
                verify_arguments(
                    analyzer,
                    func_call,
                    &arg_positions,
                    func_info,
                    name,
                    analysis_data,
                    context,
                    &template_defaults,
                    &template_replacements,
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
                Some(&template_defaults),
                Some(&template_replacements),
            );

            apply_param_out_types(
                analyzer,
                name,
                &args,
                &arg_positions,
                &func_info.params,
                context,
                &template_defaults,
                &template_replacements,
            );

            apply_assert_builtin_assertions(analyzer, name, func_call, analysis_data, context);
            apply_post_call_assertions(
                analyzer,
                func_call,
                func_info,
                context,
                &template_defaults,
                &template_replacements,
                analysis_data,
            );
            emit_non_mutation_free_magic_property_assertion_issues(
                analyzer,
                func_info,
                func_call,
                analysis_data,
            );

            if enforce_mutation_free
                && !function_call_is_mutation_free(
                    analyzer,
                    name,
                    func_info,
                    &args,
                    &arg_positions,
                    analysis_data,
                    context,
                )
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::ImpureFunctionCall,
                    format!(
                        "Cannot call an impure function {} from a mutation-free context",
                        name
                    ),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }

            if let Some(resolved_return_type) = fetched_return_type {
                let resolved_return_type = add_function_call_dataflow(
                    analyzer,
                    analysis_data,
                    FunctionLikeIdentifier::Function(func_info.name),
                    pos,
                    &arg_positions,
                    resolved_return_type,
                );
                analysis_data.set_expr_type(pos, resolved_return_type);
                return;
            }
        } else {
            // Function not found in codebase
            // Don't emit error for language constructs that look like functions
            if !is_language_construct(name)
                && !is_function_guarded_by_function_exists(context, analyzer, name)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::UndefinedFunction,
                    format!("Function {} is not defined", name),
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
            }
        }
    } else {
        if let Some(callee_type) = analysis_data.get_expr_type(callee_pos) {
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

            if let Some(return_type) = callable_validation::infer_callee_return_type(&callee_type) {
                analysis_data.set_expr_type(pos, return_type);
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
                analysis_data.set_expr_type(pos, return_type);
                return;
            }

            if callee_type.has_string()
                && !callable_validation::union_has_callable(&callee_type)
                && !has_known_literal_function_target(analyzer, &callee_type, context)
            {
                let (line, col) = analyzer.get_line_column(pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::MixedAssignment,
                    "Unable to determine return type of dynamically-invoked function",
                    analyzer.file_path,
                    pos.0,
                    pos.1,
                    line,
                    col,
                ));
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
    analysis_data.set_expr_type(pos, TUnion::mixed());
}

fn add_function_call_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    functionlike_id: FunctionLikeIdentifier,
    pos: Pos,
    arg_positions: &[Pos],
    mut return_type: TUnion,
) -> TUnion {
    let call_node =
        DataFlowNode::get_for_call(functionlike_id, make_data_flow_node_position(analyzer, pos));
    analysis_data.data_flow_graph.add_node(call_node.clone());

    for arg_pos in arg_positions {
        if let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) {
            add_default_dataflow_paths(
                &mut analysis_data.data_flow_graph,
                &arg_type.parent_nodes,
                &call_node,
            );
        }
    }

    pzoom_code_info::ttype::extend_dataflow_uniquely(
        &mut return_type.parent_nodes,
        vec![call_node],
    );

    return_type
}

fn analyze_arguments_with_callable_context(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    template_defaults: &FxHashMap<StrId, TUnion>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for arg in args {
        if is_closure_like_argument(arg) {
            continue;
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }

    let template_replacements = infer_template_replacements_from_args(
        analyzer,
        args,
        arg_positions,
        params,
        template_defaults,
        analysis_data,
        context,
    );

    for (idx, arg) in args.iter().enumerate() {
        let Some(closure_offset) = get_closure_like_argument_offset(arg) else {
            continue;
        };

        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };

        let expected_param_type = param.and_then(|param| param.get_type()).map(|param_type| {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_type.clone()
            } else {
                replace_templates_in_union(param_type, &template_replacements, template_defaults)
            }
        });

        if let Some(expected_param_type) = expected_param_type {
            if callable_validation::union_has_callable(&expected_param_type) {
                context
                    .expected_callable_arg_types
                    .insert(closure_offset, expected_param_type);
            }
        }

        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
        context.expected_callable_arg_types.remove(&closure_offset);
    }
}

fn is_closure_like_argument(arg: &Argument<'_>) -> bool {
    get_closure_like_argument_offset(arg).is_some()
}

fn get_closure_like_argument_offset(arg: &Argument<'_>) -> Option<u32> {
    match arg.value().unparenthesized() {
        Expression::Closure(closure) => Some(closure.span().start.offset),
        Expression::ArrowFunction(arrow) => Some(arrow.span().start.offset),
        _ => None,
    }
}

fn validate_direct_callable_invocation(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    pos: Pos,
) {
    let Some(callable_signature) = get_first_callable_signature(callee_type) else {
        return;
    };
    let callable_params = &callable_signature.params;

    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = callable_params
        .iter()
        .filter(|param| !param.is_optional && !param.is_variadic)
        .count();

    if !has_spread && args.len() < required_params {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments for callable, {} expected, {} provided",
                required_params,
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let accepts_unbounded = callable_params
        .last()
        .is_some_and(|param| param.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > callable_params.len() {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments for callable, {} expected, {} provided",
                callable_params.len(),
                args.len()
            ),
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    let mut callable_function_info = pzoom_code_info::FunctionLikeInfo::default();
    callable_function_info.params = callable_params
        .iter()
        .map(|param| {
            let mut param_info = ParamInfo::default();
            param_info.name = param.name.unwrap_or(StrId::EMPTY);
            param_info.param_type = Some(param.param_type.clone());
            param_info.signature_type = None;
            param_info.has_docblock_type = callable_signature.from_callable_docblock;
            param_info.is_optional = param.is_optional;
            param_info.is_variadic = param.is_variadic;
            param_info.by_ref = param.by_ref;
            param_info
        })
        .collect();
    callable_function_info.is_variadic = callable_params
        .last()
        .is_some_and(|param| param.is_variadic);

    arguments_analyzer::check_arguments_match(
        analyzer,
        args,
        arg_positions,
        &callable_function_info,
        "callable",
        analysis_data,
        context,
        None,
        None,
        pos,
        false,
        true,
    );
}

struct DirectCallableSignature {
    params: Vec<pzoom_code_info::FunctionLikeParameter>,
    // TCallable signatures generally originate from docblock callable(...) annotations.
    // TClosure signatures come from concrete closure definitions and should retain
    // scalar mismatch diagnostics.
    from_callable_docblock: bool,
}

fn get_first_callable_signature(callee_type: &TUnion) -> Option<DirectCallableSignature> {
    for atomic in &callee_type.types {
        match atomic {
            TAtomic::TCallable {
                params: Some(params),
                ..
            } => {
                return Some(DirectCallableSignature {
                    params: params.clone(),
                    from_callable_docblock: true,
                });
            }
            TAtomic::TClosure {
                params: Some(params),
                ..
            } => {
                return Some(DirectCallableSignature {
                    params: params.clone(),
                    from_callable_docblock: false,
                });
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(signature) = get_first_callable_signature(as_type) {
                    return Some(signature);
                }
            }
            TAtomic::TObjectIntersection { types } => {
                for nested_atomic in types {
                    if let Some(signature) =
                        get_first_callable_signature(&TUnion::new(nested_atomic.clone()))
                    {
                        return Some(signature);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

fn has_known_literal_function_target(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    context: &BlockContext,
) -> bool {
    callee_type.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralString { value } => {
            resolve_function(analyzer, value, false, None, context).is_some()
                || resolve_function(analyzer, value, true, None, context).is_some()
        }
        _ => false,
    })
}

/// Verify argument types against function parameter types.
fn verify_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    func_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
) {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    let span = func_call.argument_list.span();
    let arg_param_indices = arguments_analyzer::check_arguments_match(
        analyzer,
        &args,
        arg_positions,
        func_info,
        func_name,
        analysis_data,
        context,
        Some(template_defaults),
        Some(template_replacements),
        (span.start.offset, span.end.offset),
        false,
        false,
    );

    // Check if any argument is unpacked (spread operator)
    let has_spread = args.iter().any(|arg| arg.is_unpacked());

    // Count required parameters (non-optional, non-variadic)
    let required_params = func_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    // Check if we have enough arguments (skip if there's a spread, as we can't know statically)
    if !has_spread && args.len() < required_params {
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooFewArguments,
            format!(
                "Too few arguments to function {}, {} expected, {} provided",
                func_name,
                required_params,
                args.len()
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    let accepts_unbounded =
        func_info.is_variadic || func_info.params.last().is_some_and(|p| p.is_variadic);
    if !has_spread && !accepts_unbounded && args.len() > func_info.params.len() {
        let (line, col) = analyzer.get_line_column(span.start.offset);
        analysis_data.add_issue(Issue::new(
            IssueKind::TooManyArguments,
            format!(
                "Too many arguments to function {}, {} expected, {} provided",
                func_name,
                func_info.params.len(),
                args.len()
            ),
            analyzer.file_path,
            span.start.offset,
            span.end.offset,
            line,
            col,
        ));
    }

    maybe_check_builtin_callable_arity(
        analyzer,
        func_name,
        &args,
        arg_positions,
        analysis_data,
        context,
    );

    let array_filter_callback_param_type = infer_array_filter_callback_param_type_for_validation(
        analyzer,
        func_name,
        &args,
        arg_positions,
        analysis_data,
    );

    // Verify each argument type
    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        if arg.is_unpacked() {
            if let Some(arg_type) =
                arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
            {
                callable_validation::verify_unpacked_argument(
                    analyzer,
                    arg_pos,
                    &arg_type,
                    func_name,
                    func_info.no_named_arguments,
                    analysis_data,
                );
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| func_info.params.get(mapped_index))
            .or_else(|| func_info.params.last().filter(|p| p.is_variadic));

        if let Some(param) = param {
            if let Some(arg_type) =
                arguments_analyzer::get_argument_value_type(analysis_data, arg, arg_pos)
            {
                let mut effective_param = param.clone();
                if !template_defaults.is_empty() || !template_replacements.is_empty() {
                    if let Some(param_type) = param.get_type() {
                        effective_param.param_type = Some(replace_templates_in_union(
                            param_type,
                            template_replacements,
                            template_defaults,
                        ));
                    }
                }

                if param_index.unwrap_or(idx) == 1
                    && let Some(array_filter_callback_type) =
                        array_filter_callback_param_type.as_ref()
                {
                    effective_param.param_type = Some(array_filter_callback_type.clone());
                    effective_param.signature_type = Some(array_filter_callback_type.clone());
                }
                if should_relax_array_map_callback_validation(
                    analyzer,
                    func_name,
                    &args,
                    arg_positions,
                    analysis_data,
                    context,
                    arg,
                    param_index.unwrap_or(idx),
                ) {
                    maybe_relax_array_map_callback_param_for_validation(
                        func_name,
                        arg,
                        param_index.unwrap_or(idx),
                        &mut effective_param,
                    );
                }

                normalize_param_class_casing(analyzer, &mut effective_param);

                callable_validation::verify_argument_type(
                    analyzer,
                    arg,
                    arg_pos,
                    &arg_type,
                    &effective_param,
                    param_index.unwrap_or(idx),
                    func_name,
                    analysis_data,
                    context,
                );

                maybe_generalize_argument_type_after_call(
                    analyzer,
                    arg,
                    &arg_type,
                    &effective_param,
                    context,
                );
            }
        }
    }
}

fn maybe_generalize_argument_type_after_call(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    arg_type: &TUnion,
    effective_param: &ParamInfo,
    context: &mut BlockContext,
) {
    if effective_param.by_ref {
        return;
    }

    let Some(param_type) = effective_param.get_type() else {
        return;
    };

    if !arg_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNamedObject { .. }))
    {
        return;
    }

    if !param_type
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TNamedObject { .. }))
    {
        return;
    }

    let Expression::Variable(Variable::Direct(direct_var)) = arg.value().unparenthesized() else {
        return;
    };

    if direct_var.name == "this" {
        return;
    }

    let var_id = analyzer.interner.intern(direct_var.name);

    if let (
        Some(TAtomic::TNamedObject {
            name: arg_class_name,
            ..
        }),
        Some(TAtomic::TNamedObject {
            name: param_class_name,
            ..
        }),
    ) = (arg_type.get_single(), param_type.get_single())
        && arg_class_name == param_class_name
    {
        context.set_var_type_for_inference(var_id, param_type.clone());
        invalidate_property_narrowings_for_argument(analyzer, context, direct_var.name);
        return;
    }

    if let Some(intersection) =
        assertion_reconciler::intersect_union_with_union(arg_type, param_type)
        && &intersection != arg_type
    {
        context.set_var_type_for_inference(var_id, intersection);
        invalidate_property_narrowings_for_argument(analyzer, context, direct_var.name);
        return;
    }

    let mut arg_to_param = TypeComparisonResult::new();
    if !union_type_comparator::is_contained_by(
        analyzer.codebase,
        arg_type,
        param_type,
        false,
        false,
        &mut arg_to_param,
    ) {
        return;
    }

    let widened = combine_union_types(arg_type, param_type, false);
    context.set_var_type_for_inference(var_id, widened);
    invalidate_property_narrowings_for_argument(analyzer, context, direct_var.name);
}

fn invalidate_property_narrowings_for_argument(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    var_name: &str,
) {
    let prefix = format!("{}->", var_name);
    let keys_to_remove: Vec<_> = context
        .locals
        .keys()
        .copied()
        .filter(|local_id| analyzer.interner.lookup(*local_id).starts_with(&prefix))
        .collect();

    for local_id in keys_to_remove {
        context.locals.remove(&local_id);
    }
}

fn infer_array_filter_callback_param_type_for_validation(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    if !is_array_filter_function_name(func_name) {
        return None;
    }

    let callback_pos = *arg_positions.get(1)?;
    let callback_is_null = analysis_data
        .get_expr_type(callback_pos)
        .is_some_and(|callback_type| callback_type.is_null());
    if callback_is_null {
        return None;
    }

    let (value_type, key_type) = if let Some(array_pos) = arg_positions.first().copied() {
        if let Some(array_type) = analysis_data.get_expr_type(array_pos) {
            if let Some(array_info) = extract_array_like_info_from_union(&array_type) {
                let key_type = if array_info.key_type.is_nothing() {
                    TUnion::array_key()
                } else {
                    array_info.key_type
                };
                (array_info.value_type, key_type)
            } else {
                (TUnion::mixed(), TUnion::array_key())
            }
        } else {
            (TUnion::mixed(), TUnion::array_key())
        }
    } else {
        (TUnion::mixed(), TUnion::array_key())
    };

    let mut mode = 0i64;
    if let (Some(mode_pos), Some(mode_arg)) = (arg_positions.get(2).copied(), args.get(2)) {
        let raw_mode =
            infer_array_filter_mode(mode_arg, analysis_data.get_expr_type(mode_pos).as_deref());

        if let Some(raw_mode) = raw_mode {
            if !(0..=2).contains(&raw_mode) {
                let (line, col) = analyzer.get_line_column(mode_pos.0);
                analysis_data.add_issue(Issue::new(
                    IssueKind::PossiblyInvalidArgument,
                    format!(
                        "The provided 3rd argument of array_filter contains a value of {}, which will behave like 0 and filter on values only",
                        raw_mode
                    ),
                    analyzer.file_path,
                    mode_pos.0,
                    mode_pos.1,
                    line,
                    col,
                ));

                if let Some(first_arg_pos) = arg_positions.first().copied() {
                    suppress_undefined_array_filter_input_issues(analysis_data, first_arg_pos);
                }
            } else {
                mode = raw_mode;
            }
        }
    }

    let value_type = widen_literal_scalar_union_for_callable(&value_type);
    let key_type = widen_literal_scalar_union_for_callable(&key_type);

    let callback_params = match mode {
        1 => vec![
            FunctionLikeParameter {
                name: None,
                param_type: value_type,
                is_optional: false,
                is_variadic: false,
                by_ref: false,
            },
            FunctionLikeParameter {
                name: None,
                param_type: key_type,
                is_optional: false,
                is_variadic: false,
                by_ref: false,
            },
        ],
        2 => vec![FunctionLikeParameter {
            name: None,
            param_type: key_type,
            is_optional: false,
            is_variadic: false,
            by_ref: false,
        }],
        _ => vec![FunctionLikeParameter {
            name: None,
            param_type: value_type,
            is_optional: false,
            is_variadic: false,
            by_ref: false,
        }],
    };

    Some(TUnion::new(TAtomic::TCallable {
        params: Some(callback_params),
        return_type: Some(Box::new(TUnion::mixed())),
        is_pure: None,
    }))
}

fn infer_array_filter_mode(mode_arg: &Argument<'_>, mode_type: Option<&TUnion>) -> Option<i64> {
    if let Some(mode_type) = mode_type
        && let Some(mode_value) = get_single_literal_int_from_union(mode_type)
    {
        return Some(mode_value);
    }

    let normalized_name = match mode_arg.value().unparenthesized() {
        Expression::Identifier(identifier) => identifier.value().trim_start_matches('\\'),
        Expression::ConstantAccess(constant_access) => {
            constant_access.name.value().trim_start_matches('\\')
        }
        _ => return None,
    }
    .to_ascii_lowercase();

    if normalized_name == "array_filter_use_both" {
        return Some(1);
    }

    if normalized_name == "array_filter_use_key" {
        return Some(2);
    }

    None
}

fn get_single_literal_int_from_union(union: &TUnion) -> Option<i64> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}

fn suppress_undefined_array_filter_input_issues(
    analysis_data: &mut FunctionAnalysisData,
    first_arg_pos: Pos,
) {
    analysis_data.issues.retain(|issue| {
        if issue.start_offset < first_arg_pos.0 || issue.start_offset > first_arg_pos.1 {
            return true;
        }

        !matches!(
            issue.kind,
            IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
        )
    });
}

fn is_array_filter_function_name(func_name: &str) -> bool {
    func_name.eq_ignore_ascii_case("array_filter")
        || func_name.eq_ignore_ascii_case("\\array_filter")
}

fn predeclare_by_ref_argument_vars(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: Option<&str>,
    func_info: Option<&pzoom_code_info::FunctionLikeInfo>,
    args: &mago_syntax::ast::sequence::TokenSeparatedSequence<
        '_,
        mago_syntax::ast::ast::argument::Argument<'_>,
    >,
    context: &mut BlockContext,
) {
    let Some(function_name) = function_name else {
        return;
    };

    let mut next_positional_param = 0usize;
    for arg in args.iter() {
        if arg.is_unpacked() {
            continue;
        }

        let mapped_param_idx = func_info.and_then(|info| {
            resolve_param_index_for_argument(analyzer, arg, next_positional_param, &info.params)
        });
        if matches!(arg, Argument::Positional(_)) {
            next_positional_param = next_positional_param.saturating_add(1);
        }

        let by_ref_from_signature = mapped_param_idx
            .and_then(|param_idx| func_info.map(|info| info.params[param_idx].by_ref));
        let treat_as_by_ref = by_ref_from_signature.unwrap_or(false)
            || mapped_param_idx
                .is_some_and(|param_idx| is_preg_match_out_param(function_name, param_idx));
        if !treat_as_by_ref {
            continue;
        }

        if let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) =
            arg.value().unparenthesized()
        {
            let var_id = analyzer.interner.intern(direct.name);
            if !context.locals.contains_key(&var_id) {
                context.set_var_type(var_id, TUnion::mixed());
            }
        }
    }
}

fn apply_param_out_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    _arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    context: &mut BlockContext,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
) {
    let mut by_ref_var_ids = FxHashSet::default();
    let mut next_positional_param = 0usize;
    for arg in args.iter() {
        let Some(param_idx) =
            resolve_param_index_for_argument(analyzer, arg, next_positional_param, params)
        else {
            continue;
        };
        if matches!(arg, Argument::Positional(_)) {
            next_positional_param = next_positional_param.saturating_add(1);
        }
        let param = &params[param_idx];

        let treat_as_by_ref = param.by_ref || is_preg_match_out_param(function_name, param_idx);
        if !treat_as_by_ref {
            continue;
        }

        let mut resolved_out_type = if let Some(param_out_type) = &param.param_out_type {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_out_type.clone()
            } else {
                replace_templates_in_union(param_out_type, template_replacements, template_defaults)
            }
        } else if let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_type.clone()
            } else {
                replace_templates_in_union(param_type, template_replacements, template_defaults)
            }
        } else {
            TUnion::mixed()
        };

        if is_preg_match_out_param(function_name, param_idx) {
            resolved_out_type = TUnion::new(TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            });
        }

        if let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) =
            arg.value().unparenthesized()
        {
            let var_id = analyzer.interner.intern(direct.name);

            if let Some(constraint_type) = param.get_type().or(param.signature_type.as_ref()) {
                if !constraint_type.is_mixed() && var_id != StrId::THIS_VAR {
                    context.add_reference_constraint(var_id, constraint_type.clone());
                }
            }

            if var_id != StrId::THIS_VAR {
                context.set_var_type(var_id, resolved_out_type);
                by_ref_var_ids.insert(var_id);
            }
        }
    }

    if !by_ref_var_ids.is_empty() {
        context.clauses = BlockContext::remove_reconciled_clause_refs(
            &context.clauses,
            &by_ref_var_ids,
            analyzer.interner,
        )
        .0;
    }
}

fn resolve_param_index_for_argument(
    analyzer: &StatementsAnalyzer<'_>,
    arg: &Argument<'_>,
    positional_index: usize,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
) -> Option<usize> {
    let variadic_param_index = params.iter().rposition(|param| param.is_variadic);

    match arg {
        Argument::Named(named_arg) => params
            .iter()
            .position(|param| {
                let param_name = analyzer.interner.lookup(param.name);
                param_name
                    .as_ref()
                    .strip_prefix('$')
                    .unwrap_or(param_name.as_ref())
                    == named_arg.name.value
            })
            .or(variadic_param_index),
        Argument::Positional(_) => {
            if positional_index < params.len() {
                Some(positional_index)
            } else {
                variadic_param_index
            }
        }
    }
}

fn is_preg_match_out_param(function_name: &str, param_idx: usize) -> bool {
    if param_idx != 2 {
        return false;
    }

    function_name.eq_ignore_ascii_case("preg_match")
        || function_name.eq_ignore_ascii_case("\\preg_match")
        || function_name.eq_ignore_ascii_case("preg_match_all")
        || function_name.eq_ignore_ascii_case("\\preg_match_all")
}

fn apply_post_call_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    func_info: &pzoom_code_info::FunctionLikeInfo,
    context: &mut BlockContext,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if func_info.assertions.is_empty() {
        return;
    }

    for assertion in &func_info.assertions {
        let Some(param_idx) =
            find_assertion_param_index(analyzer, &func_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(argument) = func_call.argument_list.arguments.get(param_idx) else {
            continue;
        };
        let Some(param_name) = func_info
            .params
            .get(param_idx)
            .map(|param| analyzer.interner.lookup(param.name))
        else {
            continue;
        };
        let resolved_assertion_type = replace_assertion_type_templates(
            &assertion.assertion_type,
            template_replacements,
            template_defaults,
        );

        emit_undefined_docblock_class_issues_from_assertion_type(
            analyzer,
            analysis_data,
            &resolved_assertion_type,
            argument.span().start.offset,
            argument.span().end.offset,
        );

        let argument_var_key = expression_identifier::get_expression_var_key(argument.value());
        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        let Some(var_key) = argument_var_key.as_deref().and_then(|argument_var_name| {
            map_assertion_var_to_argument(
                assertion_name.as_ref(),
                param_name.as_ref(),
                argument_var_name,
            )
        }) else {
            apply_assertion_to_argument_expression(
                analyzer,
                argument.value(),
                &resolved_assertion_type,
                context,
                analysis_data,
            );
            continue;
        };

        let var_id = analyzer.interner.intern(&var_key);
        let existing_type = context
            .locals
            .get(&var_id)
            .cloned()
            .unwrap_or_else(TUnion::mixed);
        if let AssertionType::IsType(asserted_type) = &resolved_assertion_type {
            if !existing_type.is_nothing()
                && assertion_reconciler::intersect_union_with_union(&existing_type, asserted_type)
                    .is_none()
            {
                let (line, col) = analyzer.get_line_column(argument.span().start.offset);
                analysis_data.add_issue(Issue::new(
                    IssueKind::TypeDoesNotContainType,
                    format!(
                        "{} does not contain {}",
                        existing_type.get_id(Some(analyzer.interner)),
                        asserted_type.get_id(Some(analyzer.interner))
                    ),
                    analyzer.file_path,
                    argument.span().start.offset,
                    argument.span().end.offset,
                    line,
                    col,
                ));
            }
        }

        let narrowed_type =
            apply_functionlike_assertion_to_union(&existing_type, &resolved_assertion_type);
        context.locals.insert(var_id, narrowed_type);
    }
}

fn replace_assertion_type_templates(
    assertion_type: &AssertionType,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> AssertionType {
    match assertion_type {
        AssertionType::IsType(asserted_type) => AssertionType::IsType(replace_templates_in_union(
            asserted_type,
            template_replacements,
            template_defaults,
        )),
        AssertionType::IsEqual(asserted_type) => AssertionType::IsEqual(
            replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsLooselyEqual(asserted_type) => AssertionType::IsLooselyEqual(
            replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotType(asserted_type) => AssertionType::IsNotType(
            replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotEqual(asserted_type) => AssertionType::IsNotEqual(
            replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::IsNotLooselyEqual(asserted_type) => AssertionType::IsNotLooselyEqual(
            replace_templates_in_union(asserted_type, template_replacements, template_defaults),
        ),
        AssertionType::Truthy => AssertionType::Truthy,
        AssertionType::Falsy => AssertionType::Falsy,
        AssertionType::NotNull => AssertionType::NotNull,
        AssertionType::NotEmpty => AssertionType::NotEmpty,
    }
}

fn apply_assertion_to_argument_expression(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    assertion_type: &AssertionType,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    let assertion_result = assertion_finder::get_assertions(analyzer, expr, analysis_data);

    let assertion_map = match assertion_type {
        AssertionType::Truthy | AssertionType::NotEmpty | AssertionType::NotNull => {
            &assertion_result.if_true
        }
        AssertionType::Falsy => &assertion_result.if_false,
        AssertionType::IsType(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsType(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        AssertionType::IsEqual(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsEqual(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        AssertionType::IsLooselyEqual(asserted_type) if is_boolean_true_union(asserted_type) => {
            &assertion_result.if_true
        }
        AssertionType::IsLooselyEqual(asserted_type) if is_boolean_false_union(asserted_type) => {
            &assertion_result.if_false
        }
        _ => return,
    };

    if assertion_map.is_empty() {
        return;
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        assertion_map,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        false,
        None,
    );
}

fn is_boolean_true_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TTrue))
}

fn is_boolean_false_union(union: &TUnion) -> bool {
    union.is_single() && matches!(union.get_single(), Some(TAtomic::TFalse))
}

fn emit_undefined_docblock_class_issues_from_assertion_type(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    assertion_type: &AssertionType,
    start: u32,
    end: u32,
) {
    let union = match assertion_type {
        AssertionType::IsType(union)
        | AssertionType::IsEqual(union)
        | AssertionType::IsLooselyEqual(union)
        | AssertionType::IsNotType(union)
        | AssertionType::IsNotEqual(union)
        | AssertionType::IsNotLooselyEqual(union) => union,
        _ => return,
    };

    for atomic in &union.types {
        let TAtomic::TNamedObject { name, .. } = atomic else {
            continue;
        };

        if !looks_like_docblock_class_reference(analyzer.interner.lookup(*name).as_ref()) {
            continue;
        }

        let class_reference = get_docblock_class_reference(*name, analyzer);

        if matches!(class_reference, StrId::SELF | StrId::STATIC | StrId::PARENT) {
            continue;
        }

        if analyzer.codebase.get_class(class_reference).is_some() {
            continue;
        }

        let (line, col) = analyzer.get_line_column(start);
        analysis_data.add_issue(Issue::new(
            IssueKind::UndefinedDocblockClass,
            format!(
                "Docblock class {} does not exist",
                analyzer.interner.lookup(*name)
            ),
            analyzer.file_path,
            start,
            end,
            line,
            col,
        ));
    }
}

fn emit_non_mutation_free_magic_property_assertion_issues(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    func_call: &FunctionCall<'_>,
    analysis_data: &mut FunctionAnalysisData,
) {
    for assertion in function_info
        .if_true_assertions
        .iter()
        .chain(function_info.if_false_assertions.iter())
    {
        let assertion_name = analyzer.interner.lookup(assertion.var_id);
        if !assertion_name.contains("->") {
            continue;
        }

        let Some(param_idx) =
            find_assertion_param_index(analyzer, &function_info.params, assertion.var_id)
        else {
            continue;
        };
        let Some(param) = function_info.params.get(param_idx) else {
            continue;
        };
        let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) else {
            continue;
        };

        for atomic in &param_type.types {
            let TAtomic::TNamedObject { name, .. } = atomic else {
                continue;
            };
            let Some(class_info) = analyzer.codebase.get_class(*name) else {
                continue;
            };
            let Some(getter_info) = class_info.methods.get(&StrId::GET) else {
                continue;
            };

            if getter_info.is_mutation_free {
                continue;
            }

            let span = func_call.span();
            let (line, col) = analyzer.get_line_column(span.start.offset);
            analysis_data.add_issue(Issue::new(
                IssueKind::InvalidDocblock,
                format!(
                    "{}::__get is not mutation-free",
                    analyzer.interner.lookup(*name)
                ),
                analyzer.file_path,
                span.start.offset,
                span.end.offset,
                line,
                col,
            ));
        }
    }
}

fn apply_assert_builtin_assertions(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
    func_call: &FunctionCall<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if !function_name.eq_ignore_ascii_case("assert")
        && !function_name.eq_ignore_ascii_case("\\assert")
    {
        return;
    }

    let Some(first_arg) = func_call.argument_list.arguments.first() else {
        return;
    };
    if first_arg.is_unpacked() {
        return;
    }

    let assertion_result =
        assertion_finder::get_assertions(analyzer, first_arg.value(), analysis_data);

    let mut prior_truth_var_names: FxHashSet<String> = FxHashSet::default();
    if !context.clauses.is_empty() {
        let prior_clause_refs: Vec<_> = context
            .clauses
            .iter()
            .map(|clause| clause.as_ref())
            .collect();
        let prior_simplified_clauses = simplify_cnf(prior_clause_refs);
        let mut prior_cond_referenced_var_ids = FxHashSet::default();
        let (prior_truths, _) = get_truths_from_formula(
            prior_simplified_clauses.iter().collect(),
            None,
            &mut prior_cond_referenced_var_ids,
        );
        prior_truth_var_names.extend(prior_truths.into_keys());
    }

    let mut combined_clauses: Vec<_> = context
        .clauses
        .iter()
        .map(|clause| clause.as_ref())
        .collect();
    combined_clauses.extend(assertion_result.if_true_clauses.iter());

    let simplified_clauses = simplify_cnf(combined_clauses);
    let assert_conditional_id = (
        first_arg.value().start_offset() as u32,
        first_arg.value().end_offset() as u32,
    );

    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, active_truths) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(assert_conditional_id),
        &mut cond_referenced_var_ids,
    );

    let mut flattened_assertions = assertion_result.if_true.clone();
    let mut flattened_active_assertion_offsets: BTreeMap<String, FxHashSet<usize>> =
        BTreeMap::new();

    for (var_name, assertion_lists) in truths {
        let entry = flattened_assertions.entry(var_name.clone()).or_default();

        for (assertion_list_index, assertion_list) in assertion_lists.into_iter().enumerate() {
            let is_active = active_truths
                .get(&var_name)
                .is_some_and(|offsets| offsets.contains(&assertion_list_index));

            for assertion in assertion_list {
                let is_truthiness_assertion =
                    matches!(&assertion, Assertion::Truthy | Assertion::Falsy);
                let next_offset = entry.len();
                entry.push(assertion);

                if is_active {
                    if !is_truthiness_assertion {
                        continue;
                    }

                    if !prior_truth_var_names.contains(&var_name) {
                        continue;
                    }

                    let should_skip_truthiness_assertion = is_truthiness_assertion
                        && resolve_assertion_var_id(analyzer, &var_name)
                            .is_some_and(|var_id| context.is_possibly_assigned(var_id));

                    if !should_skip_truthiness_assertion {
                        flattened_active_assertion_offsets
                            .entry(var_name.clone())
                            .or_default()
                            .insert(next_offset);
                    }
                }
            }
        }
    }

    let mut changed_var_ids = FxHashSet::default();
    if !flattened_assertions.is_empty() {
        reconciler::reconcile_keyed_types(
            &flattened_assertions,
            context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            context.inside_loop,
            false,
            true,
            Some(&flattened_active_assertion_offsets),
        );
    }

    let simplified_clauses: Vec<_> = simplified_clauses
        .into_iter()
        .map(std::rc::Rc::new)
        .collect();
    context.clauses = if !changed_var_ids.is_empty() {
        BlockContext::remove_reconciled_clause_refs(
            &simplified_clauses,
            &changed_var_ids,
            analyzer.interner,
        )
        .0
    } else {
        simplified_clauses
    };
}

fn resolve_assertion_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    analyzer.interner.find(var_name).or_else(|| {
        if let Some(stripped) = var_name.strip_prefix('$') {
            analyzer.interner.find(stripped)
        } else {
            analyzer.interner.find(&format!("${}", var_name))
        }
    })
}

fn find_assertion_param_index(
    analyzer: &StatementsAnalyzer<'_>,
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    assertion_var_id: pzoom_str::StrId,
) -> Option<usize> {
    let assertion_name = analyzer.interner.lookup(assertion_var_id);

    params.iter().position(|param| {
        if param.name == assertion_var_id {
            return true;
        }

        let param_name = analyzer.interner.lookup(param.name);
        assertion_targets_param(assertion_name.as_ref(), param_name.as_ref())
    })
}

fn assertion_targets_param(assertion_name: &str, param_name: &str) -> bool {
    let normalized_assertion = assertion_name.strip_prefix('$').unwrap_or(assertion_name);
    let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name);

    if normalized_assertion == normalized_param {
        return true;
    }

    normalized_assertion
        .strip_prefix(normalized_param)
        .is_some_and(|suffix| {
            suffix.starts_with("->") || suffix.starts_with("::") || suffix.starts_with('[')
        })
}

fn map_assertion_var_to_argument(
    assertion_name: &str,
    param_name: &str,
    argument_var_name: &str,
) -> Option<String> {
    let normalized_assertion = assertion_name.strip_prefix('$').unwrap_or(assertion_name);
    let normalized_param = param_name.strip_prefix('$').unwrap_or(param_name);

    let suffix = normalized_assertion.strip_prefix(normalized_param)?;

    if suffix.is_empty() {
        return Some(argument_var_name.to_string());
    }

    if suffix.starts_with("->") || suffix.starts_with("::") || suffix.starts_with('[') {
        return Some(format!("{}{}", argument_var_name, suffix));
    }

    None
}

fn apply_functionlike_assertion_to_union(
    existing_type: &TUnion,
    assertion_type: &AssertionType,
) -> TUnion {
    let mut narrowed = match assertion_type {
        AssertionType::IsType(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsEqual(asserted_type) => {
            assertion_reconciler::intersect_union_with_union(existing_type, asserted_type)
                .unwrap_or_else(|| asserted_type.clone())
        }
        AssertionType::IsLooselyEqual(_) => existing_type.clone(),
        AssertionType::IsNotType(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotEqual(asserted_type) => subtract_union(existing_type, asserted_type),
        AssertionType::IsNotLooselyEqual(_) => existing_type.clone(),
        AssertionType::Truthy | AssertionType::NotEmpty => narrow_union_to_truthy(existing_type),
        AssertionType::Falsy => narrow_union_to_falsy(existing_type),
        AssertionType::NotNull => subtract_union(existing_type, &TUnion::new(TAtomic::TNull)),
    };

    if matches!(
        assertion_type,
        AssertionType::Truthy | AssertionType::NotEmpty
    ) {
        narrowed.is_falsable = false;
        narrowed.is_nullable = false;
    } else if matches!(assertion_type, AssertionType::NotNull) {
        narrowed.is_nullable = false;
    }

    narrowed.from_docblock = true;
    narrowed
}

fn get_docblock_class_reference(name: StrId, analyzer: &StatementsAnalyzer<'_>) -> StrId {
    let raw_name = analyzer.interner.lookup(name);
    let trimmed_name = raw_name.trim();
    let class_name = trimmed_name
        .split_once("::")
        .map_or(trimmed_name, |(class_name, _)| class_name.trim());

    if class_name.eq_ignore_ascii_case("self") {
        return StrId::SELF;
    }
    if class_name.eq_ignore_ascii_case("static") {
        return StrId::STATIC;
    }
    if class_name.eq_ignore_ascii_case("parent") {
        return StrId::PARENT;
    }

    analyzer
        .interner
        .intern(class_name.trim_start_matches('\\'))
}

fn looks_like_docblock_class_reference(raw_name: &str) -> bool {
    let trimmed_name = raw_name.trim();
    if trimmed_name.is_empty() {
        return false;
    }

    let class_name = trimmed_name
        .split_once("::")
        .map_or(trimmed_name, |(class_name, _)| class_name.trim());
    if class_name.is_empty() {
        return false;
    }

    !class_name.chars().any(|ch| {
        matches!(
            ch,
            ':' | '?' | '|' | '&' | '(' | ')' | '<' | '>' | ',' | '=' | ' ' | '\t' | '\n' | '\r'
        )
    })
}

fn narrow_union_to_truthy(existing_type: &TUnion) -> TUnion {
    let mut filtered = Vec::new();

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing => {}
            TAtomic::TBool => filtered.push(TAtomic::TTrue),
            TAtomic::TLiteralInt { value } if *value == 0 => {}
            TAtomic::TLiteralFloat { value } if *value == 0.0 => {}
            TAtomic::TLiteralString { value } if value.is_empty() || value == "0" => {}
            _ => filtered.push(atomic.clone()),
        }
    }

    if filtered.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered)
    }
}

fn narrow_union_to_falsy(existing_type: &TUnion) -> TUnion {
    let mut filtered = Vec::new();

    for atomic in &existing_type.types {
        match atomic {
            TAtomic::TNull | TAtomic::TFalse | TAtomic::TNothing => filtered.push(atomic.clone()),
            TAtomic::TBool => filtered.push(TAtomic::TFalse),
            TAtomic::TLiteralInt { value } if *value == 0 => filtered.push(atomic.clone()),
            TAtomic::TLiteralFloat { value } if *value == 0.0 => filtered.push(atomic.clone()),
            TAtomic::TLiteralString { value } if value.is_empty() || value == "0" => {
                filtered.push(atomic.clone());
            }
            TAtomic::TString
            | TAtomic::TNonEmptyString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString => filtered.push(atomic.clone()),
            _ => {}
        }
    }

    if filtered.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered)
    }
}

fn subtract_union(existing_type: &TUnion, type_to_remove: &TUnion) -> TUnion {
    let filtered_types: Vec<_> = existing_type
        .types
        .iter()
        .filter(|atomic| !type_to_remove.types.contains(atomic))
        .cloned()
        .collect();

    if filtered_types.is_empty() {
        existing_type.clone()
    } else {
        TUnion::from_types(filtered_types)
    }
}

pub(crate) fn get_template_defaults(
    func_info: &pzoom_code_info::FunctionLikeInfo,
) -> FxHashMap<StrId, TUnion> {
    let mut template_defaults = FxHashMap::default();

    for template_type in &func_info.template_types {
        template_defaults.insert(template_type.name, template_type.as_type.clone());
    }

    template_defaults
}

pub(crate) fn get_class_template_defaults(
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> FxHashMap<StrId, TUnion> {
    let mut template_defaults = FxHashMap::default();

    for template_type in &class_info.template_types {
        template_defaults.insert(template_type.name, template_type.as_type.clone());
    }

    template_defaults
}

pub(crate) fn infer_class_template_replacements_from_type_params(
    class_info: &pzoom_code_info::ClassLikeInfo,
    type_params: Option<&[TUnion]>,
) -> FxHashMap<StrId, TUnion> {
    let mut template_replacements = FxHashMap::default();
    let Some(type_params) = type_params else {
        return template_replacements;
    };

    for (idx, template_type) in class_info.template_types.iter().enumerate() {
        if let Some(type_param) = type_params.get(idx) {
            template_replacements.insert(template_type.name, type_param.clone());
        }
    }

    template_replacements
}

pub(crate) fn infer_class_template_replacements_from_extended_params(
    class_info: &pzoom_code_info::ClassLikeInfo,
) -> FxHashMap<StrId, TUnion> {
    let mut template_replacements = FxHashMap::default();

    for template_map in class_info.template_extended_params.values() {
        for (template_name, replacement) in template_map {
            if let Some(existing) = template_replacements.get(template_name) {
                template_replacements.insert(
                    *template_name,
                    combine_union_types(existing, replacement, false),
                );
            } else {
                template_replacements.insert(*template_name, replacement.clone());
            }
        }
    }

    template_replacements
}

pub(crate) fn merge_template_replacements(
    target: &mut FxHashMap<StrId, TUnion>,
    incoming: FxHashMap<StrId, TUnion>,
) {
    for (template_name, replacement) in incoming {
        if let Some(existing) = target.get(&template_name) {
            target.insert(
                template_name,
                combine_union_types(existing, &replacement, false),
            );
        } else {
            target.insert(template_name, replacement);
        }
    }
}

pub(crate) fn overlay_template_replacements(
    target: &mut FxHashMap<StrId, TUnion>,
    incoming: FxHashMap<StrId, TUnion>,
) {
    for (template_name, replacement) in incoming {
        target.insert(template_name, replacement);
    }
}

fn infer_function_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &FxHashMap<StrId, TUnion>,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> FxHashMap<StrId, TUnion> {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    infer_template_replacements_from_args(
        analyzer,
        &args,
        arg_positions,
        &func_info.params,
        template_defaults,
        analysis_data,
        context,
    )
}

pub(crate) fn infer_template_replacements_from_args(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    template_defaults: &FxHashMap<StrId, TUnion>,
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> FxHashMap<StrId, TUnion> {
    let mut template_replacements = FxHashMap::default();
    if template_defaults.is_empty() {
        return template_replacements;
    }

    for (idx, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            continue;
        }

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        let Some(arg_type) = analysis_data.get_expr_type(arg_pos) else {
            continue;
        };

        let param = if idx < params.len() {
            Some(&params[idx])
        } else {
            params.last().filter(|p| p.is_variadic)
        };

        let Some(param_type) = param.and_then(|p| p.get_type()) else {
            continue;
        };

        let arg_type = if callable_validation::union_has_callable(param_type)
            && !callable_validation::union_has_callable(&arg_type)
        {
            resolve_callable_union_for_template_inference(analyzer, &arg_type, context)
                .unwrap_or_else(|| (*arg_type).clone())
        } else {
            (*arg_type).clone()
        };

        infer_template_replacements_from_union(
            analyzer,
            param_type,
            &arg_type,
            template_defaults,
            &mut template_replacements,
        );
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

        infer_template_replacements_from_union(
            analyzer,
            param_type,
            default_type,
            template_defaults,
            &mut template_replacements,
        );
    }

    template_replacements
}

fn infer_template_replacements_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    param_type: &TUnion,
    arg_type: &TUnion,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    // When a parameter union mixes explicit non-template branches with top-level template
    // branches (e.g. `T|int`), infer templates from the unmatched argument remainder.
    let template_branches: Vec<&TAtomic> = param_type
        .types
        .iter()
        .filter(|atomic| is_top_level_template_atomic(atomic))
        .collect();
    let non_template_branches: Vec<&TAtomic> = param_type
        .types
        .iter()
        .filter(|atomic| !is_top_level_template_atomic(atomic))
        .collect();

    if !template_branches.is_empty() && !non_template_branches.is_empty() {
        let mut unmatched_arg_atomics: Vec<&TAtomic> = Vec::new();

        'arg_atomic: for arg_atomic in &arg_type.types {
            for non_template_branch in &non_template_branches {
                let mut comparison_result = TypeComparisonResult::new();
                if union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &TUnion::new((*arg_atomic).clone()),
                    &TUnion::new((*non_template_branch).clone()),
                    false,
                    false,
                    &mut comparison_result,
                ) {
                    continue 'arg_atomic;
                }
            }

            unmatched_arg_atomics.push(arg_atomic);
        }

        if !unmatched_arg_atomics.is_empty() {
            for template_branch in template_branches {
                for arg_atomic in &unmatched_arg_atomics {
                    infer_template_replacements_from_atomic(
                        analyzer,
                        template_branch,
                        arg_atomic,
                        template_defaults,
                        template_replacements,
                    );
                }
            }
            return;
        }
    }

    for param_atomic in &param_type.types {
        for arg_atomic in &arg_type.types {
            infer_template_replacements_from_atomic(
                analyzer,
                param_atomic,
                arg_atomic,
                template_defaults,
                template_replacements,
            );
        }
    }
}

fn infer_template_replacements_from_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    param_atomic: &TAtomic,
    arg_atomic: &TAtomic,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    if let TAtomic::TTemplateParam {
        as_type: arg_as_type,
        ..
    } = arg_atomic
    {
        if let TAtomic::TTemplateParam {
            name,
            as_type: param_as_type,
            ..
        } = param_atomic
        {
            let bound = template_defaults.get(name).unwrap_or(param_as_type);
            let arg_union = TUnion::new(arg_atomic.clone());
            bind_template_replacement(analyzer, *name, &arg_union, bound, template_replacements);
            infer_template_replacements_from_union(
                analyzer,
                bound,
                arg_as_type,
                template_defaults,
                template_replacements,
            );
            return;
        }

        infer_template_replacements_from_union(
            analyzer,
            &TUnion::new(param_atomic.clone()),
            arg_as_type,
            template_defaults,
            template_replacements,
        );
        return;
    }

    if let TAtomic::TTemplateParamClass { as_type, .. } = arg_atomic {
        infer_template_replacements_from_atomic(
            analyzer,
            param_atomic,
            as_type,
            template_defaults,
            template_replacements,
        );
        return;
    }

    match param_atomic {
        TAtomic::TTemplateParamClass { name, as_type, .. } => {
            let bound = template_defaults
                .get(name)
                .cloned()
                .unwrap_or_else(|| TUnion::new((**as_type).clone()));
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(analyzer, *name, &arg_union, &bound, template_replacements);

            infer_template_replacements_from_union(
                analyzer,
                &bound,
                &arg_union,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TTemplateParam { name, as_type, .. } => {
            let bound = template_defaults.get(name).unwrap_or(as_type);
            let arg_union = TUnion::new(arg_atomic.clone());

            bind_template_replacement(analyzer, *name, &arg_union, bound, template_replacements);

            infer_template_replacements_from_union(
                analyzer,
                bound,
                &arg_union,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TNamedObject {
            name,
            type_params: None,
        } if template_defaults.contains_key(name) => {
            if let Some(bound) = template_defaults.get(name) {
                bind_template_replacement(
                    analyzer,
                    *name,
                    &TUnion::new(arg_atomic.clone()),
                    bound,
                    template_replacements,
                );
            }
        }
        TAtomic::TClassString {
            as_type: Some(param_as_type),
        } => {
            if let Some(arg_as_type) = extract_class_string_atomic(analyzer, arg_atomic) {
                infer_template_replacements_from_atomic(
                    analyzer,
                    param_as_type,
                    &arg_as_type,
                    template_defaults,
                    template_replacements,
                );
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
            let Some((arg_key_type, arg_value_type)) = extract_array_like_key_value(arg_atomic)
            else {
                return;
            };

            infer_template_replacements_from_union(
                analyzer,
                key_type,
                &arg_key_type,
                template_defaults,
                template_replacements,
            );
            infer_template_replacements_from_union(
                analyzer,
                value_type,
                &arg_value_type,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            let arg_value_type = match arg_atomic {
                TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                    (**value_type).clone()
                }
                TAtomic::TArray { value_type, .. } | TAtomic::TNonEmptyArray { value_type, .. } => {
                    (**value_type).clone()
                }
                TAtomic::TKeyedArray {
                    properties,
                    fallback_value_type,
                    ..
                } => {
                    let mut combined = fallback_value_type
                        .as_ref()
                        .map(|value_type| (**value_type).clone())
                        .unwrap_or_else(TUnion::mixed);

                    for property_type in properties.values() {
                        combined = combine_union_types(&combined, property_type, false);
                    }

                    combined
                }
                _ => return,
            };

            infer_template_replacements_from_union(
                analyzer,
                value_type,
                &arg_value_type,
                template_defaults,
                template_replacements,
            );
        }
        TAtomic::TNamedObject {
            name,
            type_params: Some(param_type_params),
        } => {
            if let TAtomic::TNamedObject {
                name: arg_name,
                type_params: arg_type_params,
            } = arg_atomic
            {
                if name == arg_name
                    || (is_traversable_template_target(*name)
                        && named_object_is_traversable(analyzer, *arg_name))
                {
                    if let Some(arg_type_params) = arg_type_params {
                        for (param, arg) in param_type_params.iter().zip(arg_type_params.iter()) {
                            infer_template_replacements_from_union(
                                analyzer,
                                param,
                                arg,
                                template_defaults,
                                template_replacements,
                            );
                        }
                    }
                } else {
                    infer_named_object_template_replacements_from_extended_params(
                        analyzer,
                        *name,
                        param_type_params,
                        *arg_name,
                        arg_type_params.as_deref(),
                        template_defaults,
                        template_replacements,
                    );
                }
            }
        }
        TAtomic::TObjectIntersection { types: param_types } => {
            if let TAtomic::TObjectIntersection { types: arg_types } = arg_atomic {
                for param_type in param_types {
                    for arg_type in arg_types {
                        infer_template_replacements_from_atomic(
                            analyzer,
                            param_type,
                            arg_type,
                            template_defaults,
                            template_replacements,
                        );
                    }
                }
            } else {
                for param_type in param_types {
                    infer_template_replacements_from_atomic(
                        analyzer,
                        param_type,
                        arg_atomic,
                        template_defaults,
                        template_replacements,
                    );
                }
            }
        }
        TAtomic::TCallable {
            params: Some(param_params),
            return_type: param_return_type,
            ..
        }
        | TAtomic::TClosure {
            params: Some(param_params),
            return_type: param_return_type,
            ..
        } => {
            let (arg_params, arg_return_type) = match arg_atomic {
                TAtomic::TCallable {
                    params: Some(params),
                    return_type,
                    ..
                }
                | TAtomic::TClosure {
                    params: Some(params),
                    return_type,
                    ..
                } => (params, return_type),
                _ => return,
            };

            for (param_param, arg_param) in param_params.iter().zip(arg_params.iter()) {
                infer_template_replacements_from_union(
                    analyzer,
                    &param_param.param_type,
                    &arg_param.param_type,
                    template_defaults,
                    template_replacements,
                );
            }

            if let (Some(param_return_type), Some(arg_return_type)) =
                (param_return_type, arg_return_type)
            {
                infer_template_replacements_from_union(
                    analyzer,
                    param_return_type,
                    arg_return_type,
                    template_defaults,
                    template_replacements,
                );
            }
        }
        _ => {}
    }
}

fn infer_named_object_template_replacements_from_extended_params(
    analyzer: &StatementsAnalyzer<'_>,
    param_class_name: StrId,
    param_type_params: &[TUnion],
    arg_class_name: StrId,
    arg_type_params: Option<&[TUnion]>,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    let Some(arg_class_info) = analyzer.codebase.get_class(arg_class_name) else {
        return;
    };
    let Some(param_class_info) = analyzer.codebase.get_class(param_class_name) else {
        return;
    };
    let Some(extended_param_map) = arg_class_info
        .template_extended_params
        .get(&param_class_name)
    else {
        return;
    };

    let arg_template_defaults = get_class_template_defaults(arg_class_info);
    let arg_template_replacements =
        infer_class_template_replacements_from_type_params(arg_class_info, arg_type_params);

    for (idx, template_type) in param_class_info.template_types.iter().enumerate() {
        let Some(param_type_param) = param_type_params.get(idx) else {
            continue;
        };

        let mapped_arg_type = extended_param_map
            .get(&template_type.name)
            .cloned()
            .unwrap_or_else(|| template_type.as_type.clone());

        let resolved_arg_type =
            if arg_template_defaults.is_empty() && arg_template_replacements.is_empty() {
                mapped_arg_type
            } else {
                replace_templates_in_union(
                    &mapped_arg_type,
                    &arg_template_replacements,
                    &arg_template_defaults,
                )
            };

        infer_template_replacements_from_union(
            analyzer,
            param_type_param,
            &resolved_arg_type,
            template_defaults,
            template_replacements,
        );
    }
}

fn is_traversable_template_target(name: StrId) -> bool {
    name == StrId::TRAVERSABLE
        || name == StrId::ITERATOR
        || name == StrId::ITERATOR_AGGREGATE
        || name == StrId::GENERATOR
}

fn is_top_level_template_atomic(atomic: &TAtomic) -> bool {
    matches!(
        atomic,
        TAtomic::TTemplateParam { .. } | TAtomic::TTemplateParamClass { .. }
    )
}

fn extract_class_string_atomic(
    analyzer: &StatementsAnalyzer<'_>,
    arg_atomic: &TAtomic,
) -> Option<TAtomic> {
    match arg_atomic {
        TAtomic::TClassString {
            as_type: Some(as_type),
        } => Some((**as_type).clone()),
        TAtomic::TLiteralClassString { name } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(name),
            type_params: None,
        }),
        TAtomic::TLiteralString { value } => Some(TAtomic::TNamedObject {
            name: analyzer.interner.intern(value),
            type_params: None,
        }),
        _ => None,
    }
}

fn extract_array_like_key_value(arg_atomic: &TAtomic) -> Option<(TUnion, TUnion)> {
    match arg_atomic {
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
        } => Some(((**key_type).clone(), (**value_type).clone())),
        TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
            Some((TUnion::int(), (**value_type).clone()))
        }
        TAtomic::TKeyedArray {
            properties,
            fallback_key_type,
            fallback_value_type,
            ..
        } => {
            let mut key_union = fallback_key_type
                .as_ref()
                .map(|key_type| (**key_type).clone())
                .unwrap_or_else(TUnion::nothing);
            let mut value_union = fallback_value_type
                .as_ref()
                .map(|value_type| (**value_type).clone())
                .unwrap_or_else(TUnion::nothing);

            for (key, value) in properties {
                let key_union_part = match key {
                    pzoom_code_info::t_atomic::ArrayKey::Int(value) => {
                        TUnion::new(TAtomic::TLiteralInt { value: *value })
                    }
                    pzoom_code_info::t_atomic::ArrayKey::String(value) => {
                        TUnion::new(TAtomic::TLiteralString {
                            value: value.clone(),
                        })
                    }
                };

                key_union = combine_union_types(&key_union, &key_union_part, false);
                value_union = combine_union_types(&value_union, value, false);
            }

            if key_union.is_nothing() {
                key_union = TUnion::array_key();
            }
            if value_union.is_nothing() {
                value_union = TUnion::mixed();
            }

            Some((key_union, value_union))
        }
        TAtomic::TNamedObject { name, type_params } if *name == StrId::TRAVERSABLE => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject { name, type_params } if *name == StrId::ITERATOR => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject { name, type_params } if *name == StrId::ITERATOR_AGGREGATE => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TNamedObject { name, type_params } if *name == StrId::GENERATOR => {
            let key_type = type_params
                .as_ref()
                .and_then(|params| params.first().cloned())
                .unwrap_or_else(TUnion::array_key);
            let value_type = type_params
                .as_ref()
                .and_then(|params| params.get(1).cloned())
                .unwrap_or_else(TUnion::mixed);

            Some((key_type, value_type))
        }
        TAtomic::TTemplateParam { as_type, .. } => extract_array_like_key_value_from_union(as_type),
        TAtomic::TObjectIntersection { types } => {
            let mut key_type: Option<TUnion> = None;
            let mut value_type: Option<TUnion> = None;

            for intersection_atomic in types {
                let Some((this_key_type, this_value_type)) =
                    extract_array_like_key_value(intersection_atomic)
                else {
                    continue;
                };

                key_type = Some(if let Some(existing) = key_type {
                    combine_union_types(&existing, &this_key_type, false)
                } else {
                    this_key_type
                });
                value_type = Some(if let Some(existing) = value_type {
                    combine_union_types(&existing, &this_value_type, false)
                } else {
                    this_value_type
                });
            }

            match (key_type, value_type) {
                (Some(key_type), Some(value_type)) => Some((key_type, value_type)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn extract_array_like_key_value_from_union(arg_union: &TUnion) -> Option<(TUnion, TUnion)> {
    let mut key_type: Option<TUnion> = None;
    let mut value_type: Option<TUnion> = None;

    for atomic in &arg_union.types {
        let Some((this_key_type, this_value_type)) = extract_array_like_key_value(atomic) else {
            continue;
        };

        key_type = Some(if let Some(existing) = key_type {
            combine_union_types(&existing, &this_key_type, false)
        } else {
            this_key_type
        });
        value_type = Some(if let Some(existing) = value_type {
            combine_union_types(&existing, &this_value_type, false)
        } else {
            this_value_type
        });
    }

    match (key_type, value_type) {
        (Some(key_type), Some(value_type)) => Some((key_type, value_type)),
        _ => None,
    }
}

fn bind_template_replacement(
    analyzer: &StatementsAnalyzer<'_>,
    template_name: StrId,
    arg_type: &TUnion,
    bound: &TUnion,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    let candidate_arg_type = widen_template_argument_to_bound(arg_type, bound);

    if !bound.is_mixed() {
        let mut bound_comparison_result = TypeComparisonResult::new();
        if !union_type_comparator::is_contained_by(
            analyzer.codebase,
            &candidate_arg_type,
            bound,
            false,
            false,
            &mut bound_comparison_result,
        ) {
            return;
        }
    }

    let replacement = if let Some(existing) = template_replacements.get(&template_name) {
        combine_union_types(existing, &candidate_arg_type, false)
    } else {
        candidate_arg_type
    };

    template_replacements.insert(template_name, replacement);
}

fn widen_template_argument_to_bound(arg_type: &TUnion, bound: &TUnion) -> TUnion {
    let mut widened_types = Vec::with_capacity(arg_type.types.len());

    for atomic in &arg_type.types {
        let widened_atomic = match atomic {
            TAtomic::TLiteralInt { .. } if bound_accepts_int_like(bound) => TAtomic::TInt,
            TAtomic::TLiteralString { value }
                if bound_accepts_string_like(bound)
                    && !bound_contains_literal_string(bound, value) =>
            {
                TAtomic::TString
            }
            TAtomic::TLiteralFloat { .. } if bound_accepts_float_like(bound) => TAtomic::TFloat,
            TAtomic::TTrue | TAtomic::TFalse if bound_accepts_bool_like(bound) => TAtomic::TBool,
            _ => atomic.clone(),
        };

        if !widened_types.contains(&widened_atomic) {
            widened_types.push(widened_atomic);
        }
    }

    if widened_types.is_empty() {
        arg_type.clone()
    } else {
        TUnion::from_types(widened_types)
    }
}

fn bound_accepts_int_like(bound: &TUnion) -> bool {
    bound.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TInt
                | TAtomic::TLiteralInt { .. }
                | TAtomic::TPositiveInt
                | TAtomic::TNegativeInt
                | TAtomic::TIntRange { .. }
                | TAtomic::TArrayKey
        )
    })
}

fn bound_accepts_string_like(bound: &TUnion) -> bool {
    bound.types.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TString
                | TAtomic::TLiteralString { .. }
                | TAtomic::TLiteralClassString { .. }
                | TAtomic::TNonEmptyString
                | TAtomic::TNumericString
                | TAtomic::TNonEmptyNumericString
                | TAtomic::TLowercaseString
                | TAtomic::TNonEmptyLowercaseString
                | TAtomic::TTruthyString
                | TAtomic::TClassString { .. }
                | TAtomic::TArrayKey
        )
    })
}

fn bound_contains_literal_string(bound: &TUnion, value: &str) -> bool {
    bound.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralString { value: bound_value } => bound_value == value,
        _ => false,
    })
}

fn bound_accepts_float_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TFloat | TAtomic::TLiteralFloat { .. }))
}

fn bound_accepts_bool_like(bound: &TUnion) -> bool {
    bound
        .types
        .iter()
        .any(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
}

pub(crate) fn substitute_templates_in_union(
    union: &TUnion,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> TUnion {
    let mut replaced_types = Vec::new();

    for atomic in &union.types {
        if let Some(indexed_access_union) =
            resolve_indexed_access_template_union(atomic, template_replacements, template_defaults)
        {
            for replacement_atomic in indexed_access_union.types {
                if !replaced_types.contains(&replacement_atomic) {
                    replaced_types.push(replacement_atomic);
                }
            }
            continue;
        }

        match atomic {
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => {
                if let Some(class_replacement) = resolve_class_string_template_replacement(
                    as_type,
                    template_replacements,
                    template_defaults,
                ) {
                    for replacement_atomic in class_replacement.types {
                        let class_string_atomic = TAtomic::TClassString {
                            as_type: Some(Box::new(replacement_atomic)),
                        };
                        if !replaced_types.contains(&class_string_atomic) {
                            replaced_types.push(class_string_atomic);
                        }
                    }
                    continue;
                }

                let replaced_atomic = substitute_templates_in_atomic(
                    atomic,
                    template_replacements,
                    template_defaults,
                );
                if !replaced_types.contains(&replaced_atomic) {
                    replaced_types.push(replaced_atomic);
                }
            }
            TAtomic::TTemplateParam { name, as_type, .. } => {
                let replacement = template_replacements
                    .get(name)
                    .cloned()
                    .or_else(|| template_defaults.get(name).cloned())
                    .unwrap_or_else(|| (**as_type).clone());

                for replacement_atomic in replacement.types {
                    if !replaced_types.contains(&replacement_atomic) {
                        replaced_types.push(replacement_atomic);
                    }
                }
            }
            _ => {
                let replaced_atomic = substitute_templates_in_atomic(
                    atomic,
                    template_replacements,
                    template_defaults,
                );
                if !replaced_types.contains(&replaced_atomic) {
                    replaced_types.push(replaced_atomic);
                }
            }
        }
    }

    let mut result = if replaced_types.is_empty() {
        TUnion::mixed()
    } else {
        TUnion::from_types(replaced_types)
    };
    result.from_docblock = union.from_docblock;
    result.is_resolved = union.is_resolved;
    result.parent_nodes = union.parent_nodes.clone();
    result.ignore_nullable_issues = union.ignore_nullable_issues;
    result.ignore_falsable_issues = union.ignore_falsable_issues;
    result
}

fn resolve_class_string_template_replacement(
    as_type: &TAtomic,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> Option<TUnion> {
    match as_type {
        TAtomic::TTemplateParam { name, .. } | TAtomic::TTemplateParamClass { name, .. } => {
            template_replacements
                .get(name)
                .cloned()
                .or_else(|| template_defaults.get(name).cloned())
        }
        TAtomic::TNamedObject {
            name,
            type_params: None,
        } => template_replacements
            .get(name)
            .cloned()
            .or_else(|| template_defaults.get(name).cloned()),
        _ => None,
    }
}

fn resolve_indexed_access_template_union(
    atomic: &TAtomic,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> Option<TUnion> {
    let TAtomic::TNamedObject {
        name,
        type_params: Some(type_params),
    } = atomic
    else {
        return None;
    };

    if *name != StrId::PZOOM_INDEXED_ACCESS || type_params.len() != 2 {
        return None;
    }

    let array_type =
        substitute_templates_in_union(&type_params[0], template_replacements, template_defaults);

    Some(extract_indexed_access_value_type(&array_type))
}

fn extract_indexed_access_value_type(array_type: &TUnion) -> TUnion {
    let mut value_type = TUnion::nothing();

    for atomic in &array_type.types {
        let extracted = match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TIterable { value_type, .. } => Some((**value_type).clone()),
            TAtomic::TList { value_type } | TAtomic::TNonEmptyList { value_type } => {
                Some((**value_type).clone())
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => Some(extract_keyed_array_value_type(
                properties,
                fallback_value_type.as_deref(),
            )),
            TAtomic::TTemplateParam { as_type, .. } => {
                Some(extract_indexed_access_value_type(as_type))
            }
            _ => None,
        };

        let Some(extracted) = extracted else {
            continue;
        };

        value_type = if value_type.is_nothing() {
            extracted
        } else {
            combine_union_types(&value_type, &extracted, false)
        };
    }

    if value_type.is_nothing() {
        TUnion::mixed()
    } else {
        value_type
    }
}

pub(crate) fn replace_templates_in_union(
    union: &TUnion,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> TUnion {
    let standin_replaced =
        substitute_templates_in_union(union, template_replacements, template_defaults);

    template::inferred_type_replacer::replace(
        &standin_replaced,
        template_replacements,
        template_defaults,
    )
}

pub(crate) fn resolve_functionlike_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> Option<TUnion> {
    let return_type = function_info.get_return_type()?;
    let mut effective_template_replacements = template_replacements.clone();
    inject_fetcher_template_replacements(
        analyzer,
        function_info,
        arg_count,
        &mut effective_template_replacements,
    );

    if let Some(conditional_return_type) = &function_info.conditional_return_type {
        let mut resolved = resolve_conditional_return_type(
            analyzer,
            conditional_return_type,
            template_defaults,
            &effective_template_replacements,
            arg_count,
        );

        // Keep top-level docblock suppression flags when resolving conditional branches.
        // Psalm stores these flags on the return union itself.
        resolved.from_docblock |= return_type.from_docblock;
        resolved.ignore_nullable_issues |= return_type.ignore_nullable_issues;
        resolved.ignore_falsable_issues |= return_type.ignore_falsable_issues;

        return Some(resolved);
    }

    Some(
        if template_defaults.is_empty() && effective_template_replacements.is_empty() {
            return_type.clone()
        } else {
            replace_templates_in_union(
                return_type,
                &effective_template_replacements,
                template_defaults,
            )
        },
    )
}

fn inject_fetcher_template_replacements(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    arg_count: usize,
    template_replacements: &mut FxHashMap<StrId, TUnion>,
) {
    for template_type in &function_info.template_types {
        if template_replacements.contains_key(&template_type.name) {
            continue;
        }

        let template_name = analyzer.interner.lookup(template_type.name);
        let replacement = if template_name
            .as_ref()
            .eq_ignore_ascii_case("TFunctionArgCount")
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: arg_count as i64,
            })
        } else if template_name
            .as_ref()
            .eq_ignore_ascii_case("TPhpMajorVersion")
        {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_major_version(analyzer),
            })
        } else if template_name.as_ref().eq_ignore_ascii_case("TPhpVersionId") {
            TUnion::new(TAtomic::TLiteralInt {
                value: get_configured_php_version_id(analyzer),
            })
        } else {
            TUnion::nothing()
        };

        template_replacements.insert(template_type.name, replacement);
    }
}

fn get_configured_php_major_version(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, _, _) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    major as i64
}

fn get_configured_php_version_id(analyzer: &StatementsAnalyzer<'_>) -> i64 {
    let (major, minor, patch) = parse_php_version_tuple(analyzer.config.php_version.as_str());
    (major as i64) * 10_000 + (minor as i64) * 100 + (patch as i64)
}

fn parse_php_version_tuple(version: &str) -> (u32, u32, u32) {
    let mut parts = version.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(8);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    (major, minor, patch)
}

fn resolve_conditional_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    conditional_return_type: &pzoom_code_info::functionlike_info::ConditionalReturnType,
    template_defaults: &FxHashMap<StrId, TUnion>,
    template_replacements: &FxHashMap<StrId, TUnion>,
    arg_count: usize,
) -> TUnion {
    match &conditional_return_type.condition {
        ConditionalReturnCondition::FuncNumArgsIs { count } => {
            let selected_branch = if arg_count == *count {
                &conditional_return_type.if_true_type
            } else {
                &conditional_return_type.if_false_type
            };

            if template_defaults.is_empty() && template_replacements.is_empty() {
                selected_branch.clone()
            } else {
                replace_templates_in_union(
                    selected_branch,
                    template_replacements,
                    template_defaults,
                )
            }
        }
        ConditionalReturnCondition::TemplateIs {
            template_name,
            asserted_type,
        } => {
            let template_value = get_template_binding_case_insensitive(
                analyzer,
                template_replacements,
                *template_name,
            )
            .or_else(|| {
                get_template_binding_case_insensitive(analyzer, template_defaults, *template_name)
            });

            let asserted_type = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                asserted_type.clone()
            } else {
                replace_templates_in_union(asserted_type, template_replacements, template_defaults)
            };

            let fallback_true = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                conditional_return_type.if_true_type.clone()
            } else {
                replace_templates_in_union(
                    &conditional_return_type.if_true_type,
                    template_replacements,
                    template_defaults,
                )
            };
            let fallback_false = if template_defaults.is_empty() && template_replacements.is_empty()
            {
                conditional_return_type.if_false_type.clone()
            } else {
                replace_templates_in_union(
                    &conditional_return_type.if_false_type,
                    template_replacements,
                    template_defaults,
                )
            };

            let Some(template_value) = template_value else {
                return combine_union_types(&fallback_true, &fallback_false, false);
            };

            let mut combined: Option<TUnion> = None;
            for template_atomic in &template_value.types {
                let template_atomic_union = TUnion::new(template_atomic.clone());

                let mut branch_template_replacements = template_replacements.clone();
                branch_template_replacements.insert(*template_name, template_atomic_union.clone());

                let true_branch = replace_templates_in_union(
                    &conditional_return_type.if_true_type,
                    &branch_template_replacements,
                    template_defaults,
                );
                let false_branch = replace_templates_in_union(
                    &conditional_return_type.if_false_type,
                    &branch_template_replacements,
                    template_defaults,
                );

                let mut comparison_result = TypeComparisonResult::new();
                let definitely_true = union_type_comparator::is_contained_by(
                    analyzer.codebase,
                    &template_atomic_union,
                    &asserted_type,
                    false,
                    false,
                    &mut comparison_result,
                );

                let branch_result = if definitely_true {
                    true_branch
                } else if union_type_comparator::can_be_contained_by(
                    analyzer.codebase,
                    &template_atomic_union,
                    &asserted_type,
                ) {
                    combine_union_types(&true_branch, &false_branch, false)
                } else {
                    false_branch
                };

                combined = Some(if let Some(existing) = combined {
                    combine_union_types(&existing, &branch_result, false)
                } else {
                    branch_result
                });
            }

            combined.unwrap_or_else(|| combine_union_types(&fallback_true, &fallback_false, false))
        }
    }
}

fn get_template_binding_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    bindings: &FxHashMap<StrId, TUnion>,
    template_name: StrId,
) -> Option<TUnion> {
    if let Some(binding) = bindings.get(&template_name) {
        return Some(binding.clone());
    }

    let target = analyzer.interner.lookup(template_name);
    bindings.iter().find_map(|(candidate_name, binding)| {
        analyzer
            .interner
            .lookup(*candidate_name)
            .as_ref()
            .eq_ignore_ascii_case(target.as_ref())
            .then_some(binding.clone())
    })
}

fn substitute_templates_in_atomic(
    atomic: &TAtomic,
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
) -> TAtomic {
    match atomic {
        TAtomic::TTemplateParam { name, as_type, .. } => {
            let replacement = template_replacements
                .get(name)
                .cloned()
                .or_else(|| template_defaults.get(name).cloned())
                .unwrap_or_else(|| (**as_type).clone());

            replacement
                .get_single()
                .cloned()
                .unwrap_or_else(|| atomic.clone())
        }
        TAtomic::TArray {
            key_type,
            value_type,
        } => TAtomic::TArray {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNonEmptyArray {
            key_type,
            value_type,
        } => TAtomic::TNonEmptyArray {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TIterable {
            key_type,
            value_type,
        } => TAtomic::TIterable {
            key_type: Box::new(substitute_templates_in_union(
                key_type,
                template_replacements,
                template_defaults,
            )),
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TList { value_type } => TAtomic::TList {
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNonEmptyList { value_type } => TAtomic::TNonEmptyList {
            value_type: Box::new(substitute_templates_in_union(
                value_type,
                template_replacements,
                template_defaults,
            )),
        },
        TAtomic::TNamedObject {
            name,
            type_params: None,
        } if template_replacements.contains_key(name) || template_defaults.contains_key(name) => {
            let replacement = template_replacements
                .get(name)
                .cloned()
                .or_else(|| template_defaults.get(name).cloned())
                .unwrap_or_else(TUnion::mixed);

            replacement
                .get_single()
                .cloned()
                .unwrap_or_else(|| atomic.clone())
        }
        TAtomic::TNamedObject { name, type_params } => TAtomic::TNamedObject {
            name: *name,
            type_params: type_params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| {
                        substitute_templates_in_union(
                            param,
                            template_replacements,
                            template_defaults,
                        )
                    })
                    .collect()
            }),
        },
        TAtomic::TObjectIntersection { types } => {
            let mut replaced_types = Vec::with_capacity(types.len());
            for nested_type in types {
                let replaced_type = substitute_templates_in_atomic(
                    nested_type,
                    template_replacements,
                    template_defaults,
                );
                if !replaced_types.contains(&replaced_type) {
                    replaced_types.push(replaced_type);
                }
            }

            if replaced_types.len() == 1 {
                replaced_types.into_iter().next().unwrap()
            } else {
                TAtomic::TObjectIntersection {
                    types: replaced_types,
                }
            }
        }
        TAtomic::TKeyedArray {
            properties,
            is_list,
            sealed,
            fallback_key_type,
            fallback_value_type,
        } => {
            let mut new_properties = rustc_hash::FxHashMap::default();
            for (key, value) in properties {
                new_properties.insert(
                    key.clone(),
                    substitute_templates_in_union(value, template_replacements, template_defaults),
                );
            }

            TAtomic::TKeyedArray {
                properties: new_properties,
                is_list: *is_list,
                sealed: *sealed,
                fallback_key_type: fallback_key_type.as_ref().map(|key_type| {
                    Box::new(substitute_templates_in_union(
                        key_type,
                        template_replacements,
                        template_defaults,
                    ))
                }),
                fallback_value_type: fallback_value_type.as_ref().map(|value_type| {
                    Box::new(substitute_templates_in_union(
                        value_type,
                        template_replacements,
                        template_defaults,
                    ))
                }),
            }
        }
        TAtomic::TCallable {
            params,
            return_type,
            is_pure,
        } => TAtomic::TCallable {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: substitute_templates_in_union(
                            &param.param_type,
                            template_replacements,
                            template_defaults,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClosure {
            params,
            return_type,
            is_pure,
        } => TAtomic::TClosure {
            params: params.as_ref().map(|params| {
                params
                    .iter()
                    .map(|param| pzoom_code_info::FunctionLikeParameter {
                        name: param.name,
                        param_type: substitute_templates_in_union(
                            &param.param_type,
                            template_replacements,
                            template_defaults,
                        ),
                        is_optional: param.is_optional,
                        is_variadic: param.is_variadic,
                        by_ref: param.by_ref,
                    })
                    .collect()
            }),
            return_type: return_type.as_ref().map(|return_type| {
                Box::new(substitute_templates_in_union(
                    return_type,
                    template_replacements,
                    template_defaults,
                ))
            }),
            is_pure: *is_pure,
        },
        TAtomic::TClassString { as_type } => TAtomic::TClassString {
            as_type: as_type.as_ref().map(|as_type| {
                Box::new(substitute_templates_in_atomic(
                    as_type,
                    template_replacements,
                    template_defaults,
                ))
            }),
        },
        TAtomic::TTemplateParamClass {
            name,
            defining_entity,
            as_type,
        } => {
            if let Some(replacement) = template_replacements
                .get(name)
                .cloned()
                .or_else(|| template_defaults.get(name).cloned())
                && let Some(single_replacement) = replacement.get_single()
            {
                single_replacement.clone()
            } else {
                TAtomic::TTemplateParamClass {
                    name: *name,
                    defining_entity: *defining_entity,
                    as_type: Box::new(substitute_templates_in_atomic(
                        as_type,
                        template_replacements,
                        template_defaults,
                    )),
                }
            }
        }
        _ => atomic.clone(),
    }
}

#[derive(Clone)]
struct ArrayLikeInfo {
    key_type: TUnion,
    value_type: TUnion,
    is_list: bool,
    is_non_empty: bool,
}

pub(crate) fn infer_builtin_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let normalized_name = func_name.strip_prefix('\\').unwrap_or(func_name);

    if let Some(asserted_atomic) = get_builtin_type_check_atomic(normalized_name) {
        return infer_builtin_type_check_return_type(
            analyzer,
            arg_positions,
            analysis_data,
            asserted_atomic,
        );
    }

    if normalized_name.eq_ignore_ascii_case("utf8_encode") {
        return Some(TUnion::string());
    }

    if normalized_name.eq_ignore_ascii_case("tmpfile") {
        let mut return_type = TUnion::from_types(vec![TAtomic::TResource, TAtomic::TFalse]);
        return_type.ignore_falsable_issues = true;
        return Some(return_type);
    }

    if normalized_name.eq_ignore_ascii_case("fopen")
        && args
            .first()
            .is_some_and(|arg| named_function_call_handler::is_php_stream_literal_argument(arg))
    {
        return Some(TUnion::new(TAtomic::TResource));
    }

    if normalized_name.eq_ignore_ascii_case("getopt") {
        let getopt_value_type = TUnion::from_types(vec![
            TAtomic::TString,
            TAtomic::TFalse,
            TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
        ]);
        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(TUnion::string()),
            value_type: Box::new(getopt_value_type),
        }));
    }

    if normalized_name.eq_ignore_ascii_case("filter_input") {
        return Some(TUnion::from_types(vec![
            TAtomic::TString,
            TAtomic::TFalse,
            TAtomic::TNull,
        ]));
    }

    if normalized_name.eq_ignore_ascii_case("filter_input_array") {
        return Some(TUnion::from_types(vec![
            TAtomic::TArray {
                key_type: Box::new(TUnion::array_key()),
                value_type: Box::new(TUnion::mixed()),
            },
            TAtomic::TNull,
        ]));
    }

    if normalized_name.eq_ignore_ascii_case("explode") {
        return Some(TUnion::new(TAtomic::TNonEmptyList {
            value_type: Box::new(TUnion::string()),
        }));
    }

    if normalized_name.eq_ignore_ascii_case("str_replace") {
        return infer_str_replace_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("preg_replace") {
        return infer_preg_replace_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("preg_split") {
        return infer_preg_split_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_keys") {
        return infer_array_keys_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_values") {
        return infer_array_values_return_type(analyzer, arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_key_first")
        || normalized_name.eq_ignore_ascii_case("array_key_last")
    {
        return infer_array_key_first_last_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("range") {
        return infer_range_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("iterator_to_array") {
        return infer_iterator_to_array_return_type(analyzer, arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("count") {
        return infer_count_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_filter") {
        return infer_array_filter_return_type(args, arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_fill") {
        return infer_array_fill_return_type(arg_positions, analysis_data);
    }

    if normalized_name.eq_ignore_ascii_case("array_map") {
        return infer_array_map_return_type(analyzer, args, arg_positions, analysis_data, context);
    }

    if normalized_name.eq_ignore_ascii_case("var_export") {
        return infer_var_export_return_type(arg_positions, analysis_data);
    }

    None
}

fn get_builtin_type_check_atomic(function_name: &str) -> Option<TAtomic> {
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

fn infer_builtin_type_check_return_type(
    _analyzer: &StatementsAnalyzer<'_>,
    _arg_positions: &[Pos],
    _analysis_data: &FunctionAnalysisData,
    _asserted_atomic: TAtomic,
) -> Option<TUnion> {
    Some(TUnion::bool())
}

fn infer_array_keys_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let array_info = extract_array_like_info_from_union(&array_type)?;

    let key_type = normalize_array_key_union(&array_info.key_type);
    let atomic = if array_info.is_non_empty {
        TAtomic::TNonEmptyList {
            value_type: Box::new(key_type),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(key_type),
        }
    };

    Some(TUnion::new(atomic))
}

fn infer_array_values_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let array_info = extract_array_like_info_from_union(&array_type)?;

    if array_info.is_list {
        let (line, col) = analyzer.get_line_column(array_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::RedundantFunctionCall,
            "array_values called on a list is redundant",
            analyzer.file_path,
            array_pos.0,
            array_pos.1,
            line,
            col,
        ));
    }

    let atomic = if array_info.is_non_empty {
        TAtomic::TNonEmptyList {
            value_type: Box::new(array_info.value_type),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(array_info.value_type),
        }
    };

    Some(TUnion::new(atomic))
}

fn infer_array_key_first_last_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let array_info = extract_array_like_info_from_union(&array_type)?;

    let key_type = normalize_array_key_union(&array_info.key_type);
    if array_info.is_non_empty {
        return Some(key_type);
    }

    Some(combine_union_types(
        &key_type,
        &TUnion::new(TAtomic::TNull),
        false,
    ))
}

fn infer_var_export_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let Some(return_flag_pos) = arg_positions.get(1).copied() else {
        return Some(TUnion::void());
    };

    let return_flag_type = analysis_data.get_expr_type(return_flag_pos)?;
    match get_literal_bool_from_union(&return_flag_type) {
        Some(true) => Some(TUnion::string()),
        Some(false) => Some(TUnion::void()),
        None => Some(TUnion::from_types(vec![TAtomic::TString, TAtomic::TVoid])),
    }
}

fn infer_str_replace_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let subject_pos = arg_positions.get(2).copied()?;
    let subject_type = analysis_data.get_expr_type(subject_pos)?;
    infer_string_transform_return_type(&subject_type)
}

fn infer_preg_replace_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let subject_pos = arg_positions.get(2).copied()?;
    let subject_type = analysis_data.get_expr_type(subject_pos)?;
    let mut inferred = infer_string_transform_return_type(&subject_type)?;
    inferred.add_type(TAtomic::TNull);
    Some(inferred)
}

fn infer_string_transform_return_type(subject_type: &TUnion) -> Option<TUnion> {
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

fn infer_preg_split_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let pattern_pos = arg_positions.first().copied()?;
    let subject_pos = arg_positions.get(1).copied()?;
    let pattern_type = analysis_data.get_expr_type(pattern_pos)?;
    let subject_type = analysis_data.get_expr_type(subject_pos)?;

    if !union_contains_stringish(&pattern_type) || !union_contains_stringish(&subject_type) {
        return None;
    }

    Some(TUnion::new(TAtomic::TList {
        value_type: Box::new(TUnion::string()),
    }))
}

fn infer_range_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let start_pos = arg_positions.first().copied()?;
    let end_pos = arg_positions.get(1).copied()?;

    let start_type = analysis_data.get_expr_type(start_pos)?;
    let end_type = analysis_data.get_expr_type(end_pos)?;
    let mut value_type = combine_union_types(&start_type, &end_type, false);

    let mut normalized = Vec::new();
    for atomic in &value_type.types {
        let mapped = match atomic {
            TAtomic::TInt
            | TAtomic::TPositiveInt
            | TAtomic::TNegativeInt
            | TAtomic::TIntRange { .. }
            | TAtomic::TLiteralInt { .. } => TAtomic::TInt,
            TAtomic::TFloat | TAtomic::TLiteralFloat { .. } => TAtomic::TFloat,
            TAtomic::TString
            | TAtomic::TLiteralString { .. }
            | TAtomic::TLiteralClassString { .. }
            | TAtomic::TClassString { .. }
            | TAtomic::TNonEmptyString
            | TAtomic::TNumericString
            | TAtomic::TNonEmptyNumericString
            | TAtomic::TLowercaseString
            | TAtomic::TNonEmptyLowercaseString
            | TAtomic::TTruthyString => TAtomic::TString,
            _ => TAtomic::TMixed,
        };

        if !normalized.contains(&mapped) {
            normalized.push(mapped);
        }
    }

    if normalized.is_empty() {
        value_type = TUnion::mixed();
    } else {
        value_type = TUnion::from_types(normalized);
    }

    Some(TUnion::new(TAtomic::TNonEmptyList {
        value_type: Box::new(value_type),
    }))
}

fn infer_iterator_to_array_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let iterator_pos = arg_positions.first().copied()?;
    let iterator_type = analysis_data.get_expr_type(iterator_pos)?;
    let iterable_info = extract_iterable_like_info_from_union(analyzer, &iterator_type)?;

    let preserve_keys = arg_positions
        .get(1)
        .and_then(|pos| analysis_data.get_expr_type(*pos))
        .and_then(|ty| get_literal_bool_from_union(&ty));

    match preserve_keys {
        Some(false) => Some(TUnion::new(TAtomic::TList {
            value_type: Box::new(iterable_info.value_type),
        })),
        Some(true) | None => Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(normalize_array_key_union(&iterable_info.key_type)),
            value_type: Box::new(iterable_info.value_type),
        })),
    }
}

fn infer_count_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let value_pos = arg_positions.first().copied()?;
    let value_type = analysis_data.get_expr_type(value_pos)?;

    let mut saw_array_like = false;
    let mut saw_non_empty = false;
    let mut exact_count: Option<i64> = None;

    for atomic in &value_type.types {
        match atomic {
            TAtomic::TArray { .. } | TAtomic::TList { .. } => {
                saw_array_like = true;
                exact_count = None;
            }
            TAtomic::TNonEmptyArray { .. } | TAtomic::TNonEmptyList { .. } => {
                saw_array_like = true;
                saw_non_empty = true;
                exact_count = None;
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_key_type,
                fallback_value_type,
                ..
            } => {
                saw_array_like = true;

                if fallback_key_type.is_some() || fallback_value_type.is_some() {
                    exact_count = None;
                } else {
                    let mut fixed_count = 0i64;
                    let mut has_optional = false;

                    for property_type in properties.values() {
                        if property_type.possibly_undefined {
                            has_optional = true;
                        } else {
                            fixed_count += 1;
                        }
                    }

                    if fixed_count > 0 {
                        saw_non_empty = true;
                    }

                    if has_optional {
                        exact_count = None;
                    } else {
                        exact_count = match exact_count {
                            None => Some(fixed_count),
                            Some(existing) if existing == fixed_count => Some(existing),
                            Some(_) => None,
                        };
                    }
                }
            }
            TAtomic::TTemplateParam { as_type, .. } => {
                if let Some(info) = extract_array_like_info_from_union(as_type) {
                    saw_array_like = true;
                    if info.is_non_empty {
                        saw_non_empty = true;
                    }
                    exact_count = None;
                }
            }
            _ => {}
        }
    }

    if !saw_array_like {
        return None;
    }

    if let Some(count) = exact_count {
        return Some(TUnion::new(TAtomic::TLiteralInt { value: count }));
    }

    if saw_non_empty {
        return Some(TUnion::new(TAtomic::TPositiveInt));
    }

    Some(TUnion::new(TAtomic::TIntRange {
        min: Some(0),
        max: None,
    }))
}

fn infer_array_map_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    if args.len() < 2 || arg_positions.len() < 2 {
        return None;
    }

    let callback_type = analysis_data.get_expr_type(arg_positions[0])?;

    let mut input_array_infos = Vec::new();
    let mut callback_input_types = Vec::new();
    for arg_pos in arg_positions.iter().skip(1) {
        let array_type = analysis_data.get_expr_type(*arg_pos)?;
        let info = extract_array_like_info_from_union(&array_type)?;
        callback_input_types.push(info.value_type.clone());
        input_array_infos.push(info);
    }

    let callback_return_type = infer_array_map_callable_return_type(
        analyzer,
        &callback_type,
        &callback_input_types,
        context,
    )
    .unwrap_or_else(TUnion::mixed);

    let first_info = input_array_infos.first()?;

    if args.len() == 2 {
        if first_info.is_list {
            let atomic = if first_info.is_non_empty {
                TAtomic::TNonEmptyList {
                    value_type: Box::new(callback_return_type),
                }
            } else {
                TAtomic::TList {
                    value_type: Box::new(callback_return_type),
                }
            };
            return Some(TUnion::new(atomic));
        }

        let key_type = if first_info.key_type.is_nothing() {
            TUnion::array_key()
        } else {
            first_info.key_type.clone()
        };
        let atomic = if first_info.is_non_empty {
            TAtomic::TNonEmptyArray {
                key_type: Box::new(key_type),
                value_type: Box::new(callback_return_type),
            }
        } else {
            TAtomic::TArray {
                key_type: Box::new(key_type),
                value_type: Box::new(callback_return_type),
            }
        };
        return Some(TUnion::new(atomic));
    }

    let all_non_empty = input_array_infos.iter().all(|info| info.is_non_empty);
    let atomic = if all_non_empty {
        TAtomic::TNonEmptyList {
            value_type: Box::new(callback_return_type),
        }
    } else {
        TAtomic::TList {
            value_type: Box::new(callback_return_type),
        }
    };

    Some(TUnion::new(atomic))
}

fn infer_array_filter_return_type(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let array_pos = arg_positions.first().copied()?;
    let array_type = analysis_data.get_expr_type(array_pos)?;
    let callback_is_default = is_default_array_filter_callback(args, arg_positions, analysis_data);

    let mut filtered_types = Vec::new();

    for atomic in &array_type.types {
        let Some(filtered_atomic) = infer_array_filter_return_atomic(atomic, callback_is_default)
        else {
            continue;
        };

        if !filtered_types.contains(&filtered_atomic) {
            filtered_types.push(filtered_atomic);
        }
    }

    if filtered_types.is_empty() {
        let array_info = extract_array_like_info_from_union(&array_type)?;

        let key_type = if array_info.key_type.is_nothing() {
            TUnion::array_key()
        } else {
            normalize_array_key_union(&array_info.key_type)
        };

        let value_type = if callback_is_default {
            narrow_union_to_truthy(&array_info.value_type)
        } else {
            array_info.value_type
        };

        return Some(TUnion::new(TAtomic::TArray {
            key_type: Box::new(key_type),
            value_type: Box::new(value_type),
        }));
    }

    Some(TUnion::from_types(filtered_types))
}

fn is_default_array_filter_callback(
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> bool {
    if args.len() < 2 || arg_positions.len() < 2 {
        return true;
    }

    analysis_data
        .get_expr_type(arg_positions[1])
        .is_some_and(|callback_type| callback_type.is_null())
}

fn infer_array_filter_return_atomic(
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

            if callback_is_default && filtered_value.is_nothing() {
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
            ..
        } => {
            let mut next_properties = rustc_hash::FxHashMap::default();

            for (key, property_type) in properties {
                if callback_is_default {
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
                } else {
                    next_properties
                        .insert(key.clone(), mark_union_as_possibly_undefined(property_type));
                }
            }

            let next_fallback_value = fallback_value_type.as_ref().map(|fallback| {
                Box::new(if callback_is_default {
                    narrow_union_to_truthy(fallback)
                } else {
                    (**fallback).clone()
                })
            });

            let is_list = false;

            Some(TAtomic::TKeyedArray {
                properties: next_properties,
                is_list,
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

fn widen_literal_scalar_union_for_callable(union: &TUnion) -> TUnion {
    let mut widened = Vec::new();

    for atomic in &union.types {
        let mapped = match atomic {
            TAtomic::TLiteralInt { .. } => TAtomic::TInt,
            TAtomic::TLiteralFloat { .. } => TAtomic::TFloat,
            TAtomic::TLiteralString { .. } => TAtomic::TString,
            _ => atomic.clone(),
        };

        if !widened.contains(&mapped) {
            widened.push(mapped);
        }
    }

    if widened.is_empty() {
        union.clone()
    } else {
        TUnion::from_types(widened)
    }
}

fn infer_array_fill_return_type(
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
) -> Option<TUnion> {
    let count_pos = arg_positions.get(1).copied()?;
    let value_pos = arg_positions.get(2).copied()?;

    let count_type = analysis_data.get_expr_type(count_pos)?;
    let value_type = analysis_data
        .get_expr_type(value_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    let is_non_empty = count_type.types.iter().any(|atomic| match atomic {
        TAtomic::TLiteralInt { value } => *value > 0,
        TAtomic::TPositiveInt => true,
        TAtomic::TIntRange { min, .. } => min.is_some_and(|min| min > 0),
        _ => false,
    });

    Some(TUnion::new(if is_non_empty {
        TAtomic::TNonEmptyArray {
            key_type: Box::new(TUnion::int()),
            value_type: Box::new(value_type),
        }
    } else {
        TAtomic::TArray {
            key_type: Box::new(TUnion::int()),
            value_type: Box::new(value_type),
        }
    }))
}

fn infer_array_map_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    callback_type: &TUnion,
    callback_input_types: &[TUnion],
    context: &BlockContext,
) -> Option<TUnion> {
    let mut resolved_return_type = callable_validation::infer_callee_return_type(callback_type);

    for atomic in &callback_type.types {
        let callable_return = match atomic {
            TAtomic::TLiteralString { value } => {
                let is_fq = value.starts_with('\\');
                resolve_function(analyzer, value, is_fq, None, context)
                    .and_then(|f| resolve_callable_return_type(analyzer, f, callback_input_types))
            }
            TAtomic::TKeyedArray { properties, .. } => {
                resolve_array_callable_method(analyzer, properties, context)
                    .and_then(|m| resolve_callable_return_type(analyzer, m, callback_input_types))
            }
            _ => None,
        };

        if let Some(callable_return) = callable_return {
            resolved_return_type = Some(if let Some(existing) = resolved_return_type {
                combine_union_types(&existing, &callable_return, false)
            } else {
                callable_return
            });
        }
    }

    resolved_return_type
}

fn infer_invokable_object_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    callee_type: &TUnion,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut combined_return_type: Option<TUnion> = None;

    for atomic in &callee_type.types {
        let return_type = match atomic {
            TAtomic::TNamedObject { name, type_params } => {
                infer_invokable_named_object_return_type(
                    analyzer,
                    *name,
                    type_params.as_deref(),
                    args,
                    arg_positions,
                    analysis_data,
                    context,
                )
            }
            TAtomic::TTemplateParam { as_type, .. } => infer_invokable_object_return_type(
                analyzer,
                as_type,
                args,
                arg_positions,
                analysis_data,
                context,
            ),
            TAtomic::TObjectIntersection { types } => {
                let mut intersection_return: Option<TUnion> = None;
                for intersection_atomic in types {
                    let intersection_union = TUnion::new(intersection_atomic.clone());
                    let Some(this_return_type) = infer_invokable_object_return_type(
                        analyzer,
                        &intersection_union,
                        args,
                        arg_positions,
                        analysis_data,
                        context,
                    ) else {
                        continue;
                    };

                    intersection_return = Some(if let Some(existing) = intersection_return {
                        combine_union_types(&existing, &this_return_type, false)
                    } else {
                        this_return_type
                    });
                }

                intersection_return
            }
            _ => None,
        };

        if let Some(return_type) = return_type {
            combined_return_type = Some(if let Some(existing) = combined_return_type {
                combine_union_types(&existing, &return_type, false)
            } else {
                return_type
            });
        }
    }

    combined_return_type
}

fn infer_invokable_named_object_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    class_id: StrId,
    object_type_params: Option<&[TUnion]>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) -> Option<TUnion> {
    let class_info = analyzer.codebase.get_class(class_id)?;
    let invoke_method = class_info.methods.get(&StrId::INVOKE)?;
    invoke_method.return_type.as_ref()?;

    let mut template_defaults = get_class_template_defaults(class_info);
    template_defaults.extend(get_template_defaults(invoke_method));

    let mut template_replacements =
        infer_class_template_replacements_from_extended_params(class_info);
    overlay_template_replacements(
        &mut template_replacements,
        infer_class_template_replacements_from_type_params(class_info, object_type_params),
    );
    overlay_template_replacements(
        &mut template_replacements,
        infer_template_replacements_from_args(
            analyzer,
            args,
            arg_positions,
            &invoke_method.params,
            &template_defaults,
            analysis_data,
            context,
        ),
    );

    let callable_name = format!("{}::__invoke", analyzer.interner.lookup(class_id));
    for (idx, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            continue;
        }

        let param = if idx < invoke_method.params.len() {
            Some(&invoke_method.params[idx])
        } else {
            invoke_method
                .params
                .last()
                .filter(|param| param.is_variadic)
        };
        let Some(param) = param else {
            continue;
        };

        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));
        let Some(arg_type) = analysis_data.get_expr_type(arg_pos) else {
            continue;
        };

        let mut effective_param = param.clone();
        if let Some(param_type) = param.get_type() {
            effective_param.param_type = Some(replace_templates_in_union(
                param_type,
                &template_replacements,
                &template_defaults,
            ));
        }

        callable_validation::verify_argument_type(
            analyzer,
            arg,
            arg_pos,
            &arg_type,
            &effective_param,
            idx,
            &callable_name,
            analysis_data,
            context,
        );
    }

    let resolved_return_type = resolve_functionlike_return_type(
        analyzer,
        invoke_method,
        &template_defaults,
        &template_replacements,
        args.len(),
    )
    .unwrap_or_else(TUnion::mixed);

    Some(localize_special_class_type_union_for_callable(
        &resolved_return_type,
        class_id,
        class_info.parent_class,
    ))
}

fn localize_special_class_type_union_for_callable(
    union: &TUnion,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TUnion {
    let mut localized_types = Vec::with_capacity(union.types.len());

    for atomic in &union.types {
        let localized_atomic =
            localize_special_class_type_atomic_for_callable(atomic, self_class_id, parent_class_id);
        if !localized_types.contains(&localized_atomic) {
            localized_types.push(localized_atomic);
        }
    }

    let mut localized_union = union.clone();
    localized_union.types = localized_types;
    localized_union.is_nullable = localized_union.types.iter().any(|t| t.is_nullable());
    localized_union.is_falsable = localized_union.types.iter().any(|t| t.is_falsable());
    localized_union
}

fn localize_special_class_type_atomic_for_callable(
    atomic: &TAtomic,
    self_class_id: StrId,
    parent_class_id: Option<StrId>,
) -> TAtomic {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            let localized_name = if *name == StrId::SELF || *name == StrId::STATIC {
                self_class_id
            } else if *name == StrId::PARENT {
                parent_class_id.unwrap_or(StrId::PARENT)
            } else {
                *name
            };

            TAtomic::TNamedObject {
                name: localized_name,
                type_params: type_params.as_ref().map(|params| {
                    params
                        .iter()
                        .map(|param| {
                            localize_special_class_type_union_for_callable(
                                param,
                                self_class_id,
                                parent_class_id,
                            )
                        })
                        .collect()
                }),
            }
        }
        TAtomic::TObjectIntersection { types } => TAtomic::TObjectIntersection {
            types: types
                .iter()
                .map(|nested_type| {
                    localize_special_class_type_atomic_for_callable(
                        nested_type,
                        self_class_id,
                        parent_class_id,
                    )
                })
                .collect(),
        },
        _ => atomic.clone(),
    }
}

fn resolve_callable_return_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    arg_types: &[TUnion],
) -> Option<TUnion> {
    function_info.get_return_type()?;
    let template_defaults = get_template_defaults(function_info);

    let mut template_replacements = FxHashMap::default();
    for (idx, param) in function_info.params.iter().enumerate() {
        let Some(param_type) = param.get_type() else {
            continue;
        };
        let Some(arg_type) = arg_types.get(idx) else {
            continue;
        };

        infer_template_replacements_from_union(
            analyzer,
            param_type,
            arg_type,
            &template_defaults,
            &mut template_replacements,
        );
    }

    let resolved_return_type = resolve_functionlike_return_type(
        analyzer,
        function_info,
        &template_defaults,
        &template_replacements,
        arg_types.len(),
    )?;

    if let Some(self_class_id) = function_info.declaring_class {
        let parent_class_id = analyzer
            .codebase
            .get_class(self_class_id)
            .and_then(|class_info| class_info.parent_class);

        return Some(localize_special_class_type_union_for_callable(
            &resolved_return_type,
            self_class_id,
            parent_class_id,
        ));
    }

    Some(resolved_return_type)
}

fn resolve_array_callable_method<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    properties: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, TUnion>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let first = properties.get(&pzoom_code_info::ArrayKey::Int(0))?;
    let second = properties.get(&pzoom_code_info::ArrayKey::Int(1))?;

    let method_name = get_literal_string_from_union(second)?;
    let class_id = get_callable_class_from_union(analyzer, first, context)?;

    let class_info = analyzer.codebase.get_class(class_id)?;
    get_method_info_case_insensitive(analyzer, class_info, method_name)
}

fn get_callable_class_from_union(
    analyzer: &StatementsAnalyzer<'_>,
    class_union: &TUnion,
    context: &BlockContext,
) -> Option<StrId> {
    let mut class_id = None;

    for atomic in &class_union.types {
        let atomic_class_id = match atomic {
            TAtomic::TLiteralClassString { name } => {
                let class_name = name.strip_prefix('\\').unwrap_or(name);
                Some(analyzer.interner.intern(class_name))
            }
            TAtomic::TLiteralString { value } => {
                let class_name = value.strip_prefix('\\').unwrap_or(value);
                resolve_class_name_for_callable(analyzer, class_name, context)
            }
            TAtomic::TNamedObject { name, .. } => Some(*name),
            TAtomic::TClassString {
                as_type: Some(as_type),
            } => get_named_class_from_atomic(as_type),
            TAtomic::TTemplateParam { as_type, .. } => {
                get_callable_class_from_union(analyzer, as_type, context)
            }
            TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_from_atomic(as_type),
            _ => None,
        }?;

        if let Some(existing) = class_id {
            if existing != atomic_class_id {
                return None;
            }
        } else {
            class_id = Some(atomic_class_id);
        }
    }

    class_id
}

fn get_named_class_from_atomic(atomic: &TAtomic) -> Option<StrId> {
    match atomic {
        TAtomic::TNamedObject { name, .. } => Some(*name),
        TAtomic::TTemplateParam { as_type, .. } => {
            if as_type.is_single() {
                get_named_class_from_atomic(as_type.get_single()?)
            } else {
                None
            }
        }
        TAtomic::TTemplateParamClass { as_type, .. } => get_named_class_from_atomic(as_type),
        _ => None,
    }
}

fn get_method_info_case_insensitive<'a>(
    analyzer: &StatementsAnalyzer<'_>,
    class_info: &'a pzoom_code_info::ClassLikeInfo,
    method_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    let method_id = analyzer.interner.intern(method_name);

    if let Some(method_info) = class_info.methods.get(&method_id) {
        return Some(method_info);
    }

    class_info
        .methods
        .iter()
        .find_map(|(stored_id, method_info)| {
            analyzer
                .interner
                .lookup(*stored_id)
                .as_ref()
                .eq_ignore_ascii_case(method_name)
                .then_some(method_info)
        })
}

fn resolve_class_name_for_callable(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
    context: &BlockContext,
) -> Option<StrId> {
    let normalized = class_name.strip_prefix('\\').unwrap_or(class_name);
    let class_id = analyzer.interner.intern(normalized);

    if analyzer.codebase.classlike_infos.contains_key(&class_id) {
        return Some(class_id);
    }

    if let Some(ns_id) = context.namespace {
        let ns = analyzer.interner.lookup(ns_id);
        let qualified = format!("{}\\{}", ns, normalized);
        let qualified_id = analyzer.interner.intern(&qualified);
        if analyzer
            .codebase
            .classlike_infos
            .contains_key(&qualified_id)
        {
            return Some(qualified_id);
        }
    }

    None
}

fn get_literal_string_from_union(union: &TUnion) -> Option<&str> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TLiteralString { value }) => Some(value.as_str()),
        _ => None,
    }
}

fn resolve_callable_union_for_template_inference(
    analyzer: &StatementsAnalyzer<'_>,
    arg_type: &TUnion,
    context: &BlockContext,
) -> Option<TUnion> {
    let mut callable_union: Option<TUnion> = None;

    for atomic in &arg_type.types {
        let callable_atomic = match atomic {
            TAtomic::TCallable { .. } | TAtomic::TClosure { .. } => Some(atomic.clone()),
            TAtomic::TLiteralString { value } => {
                let cleaned = value.strip_prefix('\\').unwrap_or(value);

                if let Some((class_name, method_name)) = cleaned.split_once("::") {
                    let class_id = resolve_class_name_for_callable(analyzer, class_name, context)?;
                    let class_info = analyzer.codebase.get_class(class_id)?;
                    let method_id = analyzer.interner.intern(method_name);
                    class_info
                        .methods
                        .get(&method_id)
                        .map(functionlike_to_callable_atomic)
                } else {
                    let is_fq = value.starts_with('\\');
                    resolve_function(analyzer, value, is_fq, None, context)
                        .map(functionlike_to_callable_atomic)
                }
            }
            TAtomic::TKeyedArray { properties, .. } => {
                resolve_array_callable_method(analyzer, properties, context)
                    .map(functionlike_to_callable_atomic)
            }
            _ => None,
        };

        if let Some(callable_atomic) = callable_atomic {
            callable_union = Some(if let Some(existing) = callable_union {
                combine_union_types(&existing, &TUnion::new(callable_atomic), false)
            } else {
                TUnion::new(callable_atomic)
            });
        }
    }

    callable_union
}

fn functionlike_to_callable_atomic(function_info: &pzoom_code_info::FunctionLikeInfo) -> TAtomic {
    let params = function_info
        .params
        .iter()
        .map(|param| pzoom_code_info::FunctionLikeParameter {
            name: Some(param.name),
            param_type: param.get_type().cloned().unwrap_or_else(TUnion::mixed),
            is_optional: param.is_optional,
            is_variadic: param.is_variadic,
            by_ref: param.by_ref,
        })
        .collect::<Vec<_>>();

    TAtomic::TCallable {
        params: Some(params),
        return_type: function_info.get_return_type().cloned().map(Box::new),
        is_pure: Some(function_info.is_pure || function_info.is_mutation_free),
    }
}

fn union_contains_non_pure_callable(union: &TUnion) -> bool {
    union.types.iter().any(atomic_is_non_pure_callable)
}

fn atomic_is_non_pure_callable(atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::TCallable { is_pure, .. } | TAtomic::TClosure { is_pure, .. } => {
            !matches!(is_pure, Some(true))
        }
        TAtomic::TTemplateParam { as_type, .. } => union_contains_non_pure_callable(as_type),
        TAtomic::TObjectIntersection { types } => types.iter().any(atomic_is_non_pure_callable),
        _ => false,
    }
}

fn extract_array_like_info_from_union(union: &TUnion) -> Option<ArrayLikeInfo> {
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
            let value_type =
                extract_keyed_array_value_type(properties, fallback_value_type.as_deref());

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

fn extract_iterable_like_info_from_union(
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
        TAtomic::TNamedObject { name, type_params } => {
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

fn extract_keyed_array_value_type(
    properties: &rustc_hash::FxHashMap<pzoom_code_info::ArrayKey, TUnion>,
    fallback_value_type: Option<&TUnion>,
) -> TUnion {
    let mut value_type = fallback_value_type.cloned().unwrap_or_else(TUnion::nothing);

    for property_type in properties.values() {
        value_type = if value_type.is_nothing() {
            property_type.clone()
        } else {
            combine_union_types(&value_type, property_type, false)
        };
    }

    if value_type.is_nothing() {
        TUnion::mixed()
    } else {
        value_type
    }
}

fn named_object_is_traversable(analyzer: &StatementsAnalyzer<'_>, name: StrId) -> bool {
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

fn normalize_array_key_union(key_type: &TUnion) -> TUnion {
    if key_type.is_nothing() {
        return TUnion::nothing();
    }

    assertion_reconciler::intersect_union_with_union(key_type, &TUnion::array_key())
        .unwrap_or_else(TUnion::array_key)
}

fn get_literal_bool_from_union(union: &TUnion) -> Option<bool> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TTrue) => Some(true),
        Some(TAtomic::TFalse) => Some(false),
        _ => None,
    }
}

fn maybe_check_builtin_callable_arity(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
) {
    if !func_name.eq_ignore_ascii_case("array_map") {
        return;
    }

    if args.len() < 2 || args.iter().skip(1).any(|arg| arg.is_unpacked()) {
        return;
    }

    let callback_pos = if let Some(pos) = arg_positions.first().copied() {
        pos
    } else {
        return;
    };

    let Some(callback_type) = analysis_data.get_expr_type(callback_pos) else {
        return;
    };

    let callback_arity = args.len().saturating_sub(1);
    match callable_arity_status(analyzer, &callback_type, callback_arity, context) {
        CallableArityStatus::TooFew { required } => {
            let (line, col) = analyzer.get_line_column(callback_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooFewArguments,
                format!(
                    "Too few arguments for callable passed to array_map, {} expected, {} provided",
                    required, callback_arity
                ),
                analyzer.file_path,
                callback_pos.0,
                callback_pos.1,
                line,
                col,
            ));
        }
        CallableArityStatus::TooMany { max } => {
            let (line, col) = analyzer.get_line_column(callback_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooManyArguments,
                format!(
                    "Too many arguments for callable passed to array_map, {} expected, {} provided",
                    max, callback_arity
                ),
                analyzer.file_path,
                callback_pos.0,
                callback_pos.1,
                line,
                col,
            ));
        }
        CallableArityStatus::Supported | CallableArityStatus::Unknown => {}
    }
}

enum CallableArityStatus {
    Supported,
    TooFew { required: usize },
    TooMany { max: usize },
    Unknown,
}

fn callable_arity_status(
    analyzer: &StatementsAnalyzer<'_>,
    callback_type: &TUnion,
    arity: usize,
    context: &BlockContext,
) -> CallableArityStatus {
    let mut saw_unknown = false;
    let mut saw_known = false;
    let mut min_required_above: Option<usize> = None;
    let mut max_allowed_below: Option<usize> = None;

    for atomic in &callback_type.types {
        match atomic {
            TAtomic::TNull => {}
            TAtomic::TCallable { params, .. } | TAtomic::TClosure { params, .. } => {
                let Some(params) = params.as_ref() else {
                    saw_unknown = true;
                    continue;
                };

                saw_known = true;
                let required_count = params
                    .iter()
                    .filter(|param| !param.is_optional && !param.is_variadic)
                    .count();
                let param_count = params.len();
                let is_variadic = params.last().is_some_and(|param| param.is_variadic);

                if params_accept_arity(required_count, param_count, is_variadic, arity) {
                    return CallableArityStatus::Supported;
                }

                if arity < required_count {
                    min_required_above = Some(
                        min_required_above
                            .map_or(required_count, |existing| existing.min(required_count)),
                    );
                } else if !is_variadic && arity > param_count {
                    max_allowed_below = Some(
                        max_allowed_below.map_or(param_count, |existing| existing.max(param_count)),
                    );
                }
            }
            TAtomic::TLiteralString { value } => {
                let Some(function_info) = resolve_function(analyzer, value, false, None, context)
                else {
                    saw_unknown = true;
                    continue;
                };

                saw_known = true;
                let required_count = function_info
                    .params
                    .iter()
                    .filter(|param| !param.is_optional && !param.is_variadic)
                    .count();
                let param_count = function_info.params.len();
                let is_variadic = function_info
                    .params
                    .last()
                    .is_some_and(|param| param.is_variadic);

                if params_accept_arity(required_count, param_count, is_variadic, arity) {
                    return CallableArityStatus::Supported;
                }

                if arity < required_count {
                    min_required_above = Some(
                        min_required_above
                            .map_or(required_count, |existing| existing.min(required_count)),
                    );
                } else if !is_variadic && arity > param_count {
                    max_allowed_below = Some(
                        max_allowed_below.map_or(param_count, |existing| existing.max(param_count)),
                    );
                }
            }
            _ => {
                saw_unknown = true;
            }
        }
    }

    if min_required_above.is_some() && max_allowed_below.is_none() {
        return CallableArityStatus::TooFew {
            required: min_required_above.unwrap_or(arity + 1),
        };
    }

    if max_allowed_below.is_some() && min_required_above.is_none() {
        return CallableArityStatus::TooMany {
            max: max_allowed_below.unwrap_or(arity.saturating_sub(1)),
        };
    }

    if saw_known || saw_unknown {
        CallableArityStatus::Unknown
    } else {
        CallableArityStatus::Supported
    }
}

fn params_accept_arity(
    required_count: usize,
    param_count: usize,
    variadic: bool,
    arity: usize,
) -> bool {
    arity >= required_count && (variadic || arity <= param_count)
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

fn normalize_param_class_casing(
    analyzer: &StatementsAnalyzer<'_>,
    param: &mut pzoom_code_info::functionlike_info::ParamInfo,
) {
    if let Some(param_type) = param.param_type.as_mut() {
        normalize_union_class_casing(analyzer, param_type);
    }

    if let Some(signature_type) = param.signature_type.as_mut() {
        normalize_union_class_casing(analyzer, signature_type);
    }

    if let Some(param_out_type) = param.param_out_type.as_mut() {
        normalize_union_class_casing(analyzer, param_out_type);
    }
}

fn normalize_union_class_casing(analyzer: &StatementsAnalyzer<'_>, union: &mut TUnion) {
    for atomic in &mut union.types {
        normalize_atomic_class_casing(analyzer, atomic);
    }
}

fn normalize_atomic_class_casing(analyzer: &StatementsAnalyzer<'_>, atomic: &mut TAtomic) {
    match atomic {
        TAtomic::TNamedObject { name, type_params } => {
            if analyzer.codebase.get_class(*name).is_none() {
                let requested = analyzer.interner.lookup(*name);
                if let Some(actual_id) = find_class_case_insensitive(analyzer, requested.as_ref()) {
                    *name = actual_id;
                }
            }

            if let Some(type_params) = type_params {
                for type_param in type_params {
                    normalize_union_class_casing(analyzer, type_param);
                }
            }
        }
        TAtomic::TClassString {
            as_type: Some(as_type),
        }
        | TAtomic::TTemplateParamClass { as_type, .. } => {
            normalize_atomic_class_casing(analyzer, as_type);
        }
        TAtomic::TTemplateParam { as_type, .. } => {
            normalize_union_class_casing(analyzer, as_type);
        }
        _ => {}
    }
}

fn find_class_case_insensitive(
    analyzer: &StatementsAnalyzer<'_>,
    class_name: &str,
) -> Option<StrId> {
    analyzer
        .codebase
        .classlike_infos
        .keys()
        .copied()
        .find(|class_id| {
            analyzer
                .interner
                .lookup(*class_id)
                .as_ref()
                .eq_ignore_ascii_case(class_name)
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
fn resolve_function<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    name: &str,
    is_fully_qualified: bool,
    name_offset: Option<u32>,
    context: &BlockContext,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    if let Some(offset) = name_offset
        && let Some(resolved_name) = analyzer.get_resolved_name(offset)
    {
        if let Some(func_info) = analyzer.codebase.get_function(resolved_name) {
            return Some(func_info);
        }

        let resolved_name = analyzer.interner.lookup(resolved_name);
        if let Some(func_info) =
            get_function_case_insensitive(analyzer, resolved_name.trim_start_matches('\\'))
        {
            return Some(func_info);
        }
    }

    if is_fully_qualified {
        // Strip leading backslash and look up directly
        let clean_name = name.strip_prefix('\\').unwrap_or(name);
        let func_id = analyzer.interner.intern(clean_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }

        return get_function_case_insensitive(analyzer, clean_name);
    }

    // Try namespace-qualified lookup first
    if let Some(ns_id) = context.namespace {
        let ns_str = analyzer.interner.lookup(ns_id);
        let qualified_name = format!("{}\\{}", ns_str, name);
        let func_id = analyzer.interner.intern(&qualified_name);
        if let Some(func_info) = analyzer.codebase.get_function(func_id) {
            return Some(func_info);
        }

        if let Some(func_info) = get_function_case_insensitive(analyzer, &qualified_name) {
            return Some(func_info);
        }
    }

    // Fall back to global namespace
    let func_id = analyzer.interner.intern(name);
    if let Some(func_info) = analyzer.codebase.get_function(func_id) {
        return Some(func_info);
    }

    get_function_case_insensitive(analyzer, name)
}

fn get_function_case_insensitive<'a>(
    analyzer: &'a StatementsAnalyzer<'_>,
    function_name: &str,
) -> Option<&'a pzoom_code_info::FunctionLikeInfo> {
    analyzer
        .codebase
        .functionlike_infos
        .iter()
        .find_map(|(func_id, func_info)| {
            analyzer
                .interner
                .lookup(*func_id)
                .trim_start_matches('\\')
                .eq_ignore_ascii_case(function_name.trim_start_matches('\\'))
                .then_some(func_info)
        })
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
    func_name: &str,
    function_info: &pzoom_code_info::FunctionLikeInfo,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
) -> bool {
    if function_is_mutation_free(function_info) {
        return true;
    }

    if is_array_map_function_name(func_name) {
        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 0);
    }

    if is_array_filter_function_name(func_name) {
        if args.len() < 2 {
            return true;
        }

        return callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 1);
    }

    false
}

fn callback_argument_is_pure(
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
    let Some(callback_type) = analysis_data.get_expr_type(callback_pos) else {
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
                saw_non_null_candidate = true;
                return false;
            }
        }
    }

    !saw_non_null_candidate || saw_callable_candidate
}

fn callable_union_is_pure(union: &TUnion) -> bool {
    let mut saw_non_null_candidate = false;
    let mut saw_callable_candidate = false;

    for atomic in &union.types {
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
                saw_non_null_candidate = true;
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
    let Some(function_info) = analyzer.function_info else {
        return false;
    };

    if function_info.is_pure || function_info.is_mutation_free {
        return true;
    }

    if function_info.is_static {
        return false;
    }

    if let Some(class_id) = function_info.declaring_class {
        return analyzer
            .codebase
            .get_class(class_id)
            .is_some_and(|class_info| class_info.is_immutable);
    }

    false
}

fn record_named_function_callsite_argument_types(
    function_id: StrId,
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) {
    for (index, arg_pos) in arg_positions.iter().enumerate() {
        let Some(arg_type) = analysis_data.get_expr_type(*arg_pos) else {
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
fn is_language_construct(name: &str) -> bool {
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

fn is_array_map_function_name(func_name: &str) -> bool {
    func_name.eq_ignore_ascii_case("array_map") || func_name.eq_ignore_ascii_case("\\array_map")
}

fn should_relax_array_map_callback_validation(
    analyzer: &StatementsAnalyzer<'_>,
    func_name: &str,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
    arg: &Argument<'_>,
    param_index: usize,
) -> bool {
    if !is_array_map_function_name(func_name) || param_index != 0 {
        return false;
    }

    if matches!(
        arg.value().unparenthesized(),
        Expression::Closure(_) | Expression::ArrowFunction(_)
    ) {
        return false;
    }

    callback_argument_is_pure(analyzer, args, arg_positions, analysis_data, context, 0)
}

fn maybe_relax_array_map_callback_param_for_validation(
    func_name: &str,
    arg: &Argument<'_>,
    param_index: usize,
    effective_param: &mut ParamInfo,
) {
    if param_index != 0 || !is_array_map_function_name(func_name) {
        return;
    }

    if matches!(
        arg.value().unparenthesized(),
        Expression::Closure(_) | Expression::ArrowFunction(_)
    ) {
        return;
    }

    let Some(signature_type) = effective_param.signature_type.clone() else {
        return;
    };

    if !callable_validation::union_has_callable(&signature_type) {
        return;
    }

    effective_param.param_type = Some(signature_type);
    effective_param.has_docblock_type = false;
}

fn is_function_guarded_by_function_exists(
    context: &BlockContext,
    analyzer: &StatementsAnalyzer<'_>,
    function_name: &str,
) -> bool {
    let key = function_exists_assertion_key(function_name);
    let key_id = analyzer.interner.intern(&key);

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
