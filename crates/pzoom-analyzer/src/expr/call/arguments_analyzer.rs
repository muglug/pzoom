//! Multiple arguments analyzer.
//!
//! This module analyzes all arguments in a function/method call.
//! Argument type verification against function parameters is handled
//! by the individual call analyzers (function_call_analyzer, method_call_analyzer, etc.)
//! which have access to the function signature.

use super::function_call_analyzer;

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::{Argument, ArgumentList};
use mago_syntax::ast::ast::call::{Call, FunctionCall};
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::VarName;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::{
    DataFlowNode, FunctionLikeIdentifier, FunctionLikeInfo, FunctionLikeParameter, GraphKind,
    Issue, IssueKind, PathKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::data_flow::make_data_flow_node_position;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

use super::{argument_analyzer, callable_validation};
use pzoom_code_info::TemplateResult;

/// Build the dataflow identifier for a function-like (Hakana threads
/// `FunctionLikeIdentifier` everywhere; pzoom reconstructs it from storage).
fn functionlike_id_for_info(info: &FunctionLikeInfo) -> pzoom_code_info::FunctionLikeIdentifier {
    if let Some(declaring_class) = info.declaring_class {
        pzoom_code_info::FunctionLikeIdentifier::Method(declaring_class, info.name)
    } else {
        pzoom_code_info::FunctionLikeIdentifier::Function(info.name)
    }
}

/// The `(functionlike_id, call_pos, specialize_taint)` triple `verify_type`
/// threads into argument dataflow. Hakana specializes calls to nearly every
/// function-like (plain functions, pure functions, non-static methods, and
/// static methods without static-field access); pzoom mirrors that with a
/// `true` default. The exception Hakana carves out — static methods that
/// access static fields (memoization caches), where taints must flow across
/// call sites — is keyed off `static_field_access` when the scanner records
/// it.
fn call_dataflow_for_info(
    info: &FunctionLikeInfo,
    call_pos: Pos,
) -> (pzoom_code_info::FunctionLikeIdentifier, Pos, bool) {
    (
        functionlike_id_for_info(info),
        call_pos,
        info.taints.specialize_call,
    )
}

/// Like [`call_dataflow_for_info`], but the argument node belongs to the
/// class named at the call site (the receiver / static class), so taints
/// flow `Called::method#N → Declaring::method#N` (Hakana's
/// `declaring_method_id` edge in `argument_analyzer::add_dataflow`).
pub(crate) fn call_dataflow_for_method_call(
    called_class: pzoom_str::StrId,
    info: &FunctionLikeInfo,
    call_pos: Pos,
) -> (pzoom_code_info::FunctionLikeIdentifier, Pos, bool) {
    (
        pzoom_code_info::FunctionLikeIdentifier::Method(called_class, info.name),
        call_pos,
        info.taints.specialize_call,
    )
}

/// Analyze all arguments in a function/method call.
///
/// This analyzes each argument expression to determine its type.
/// The specialized call analyzers (function_call_analyzer, method_call_analyzer,
/// static_call_analyzer) are responsible for:
/// - Verifying argument count matches parameter count
/// - Handling default parameter values
/// - Handling variadic parameters
/// - Verifying argument types against parameter types
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    argument_list: &ArgumentList<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for arg in argument_list.arguments.iter() {
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }
}

pub(crate) fn check_arguments_match(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    function_info: &FunctionLikeInfo,
    callable_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &BlockContext,
    template_result: Option<&TemplateResult>,
    call_pos: Pos,
    check_counts: bool,
    check_types: bool,
) -> Vec<Option<usize>> {
    let has_spread = args.iter().any(|arg| arg.is_unpacked());
    let required_params = function_info
        .params
        .iter()
        .filter(|p| !p.is_optional && !p.is_variadic)
        .count();

    if check_counts {
        if !has_spread && args.len() < required_params {
            let issue_pos = arg_positions.first().copied().unwrap_or(call_pos);
            let (line, col) = analyzer.get_line_column(issue_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooFewArguments,
                format!(
                    "Too few arguments to function {}, {} expected, {} provided",
                    callable_name,
                    required_params,
                    args.len()
                ),
                analyzer.file_path,
                issue_pos.0,
                issue_pos.1,
                line,
                col,
            ));
        }
    }

    let accepts_unbounded =
        function_info.is_variadic || function_info.params.last().is_some_and(|p| p.is_variadic);
    if check_counts {
        if !has_spread && !accepts_unbounded && args.len() > function_info.params.len() {
            let issue_pos = arg_positions
                .get(function_info.params.len())
                .copied()
                .unwrap_or(call_pos);
            let (line, col) = analyzer.get_line_column(issue_pos.0);
            analysis_data.add_issue(Issue::new(
                IssueKind::TooManyArguments,
                format!(
                    "Too many arguments to function {}, {} expected, {} provided",
                    callable_name,
                    function_info.params.len(),
                    args.len()
                ),
                analyzer.file_path,
                issue_pos.0,
                issue_pos.1,
                line,
                col,
            ));
        }
    }

    let arg_param_indices = resolve_argument_param_indices(
        analyzer,
        args,
        arg_positions,
        &function_info.params,
        callable_name,
        function_info.no_named_arguments,
        analysis_data,
    );

    if check_types {
        for (idx, arg) in args.iter().enumerate() {
            let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

            if arg.is_unpacked() {
                if let Some(arg_type) = get_argument_value_type(analysis_data, arg, arg_pos) {
                    argument_analyzer::verify_unpacked_argument(
                        analyzer,
                        arg_pos,
                        &arg_type,
                        callable_name,
                        function_info.no_named_arguments,
                        analysis_data,
                    );

                    // Whole-program (taint) mode: the unpacked array's values
                    // flow into the mapped (or trailing variadic) parameter
                    // (Psalm verifies and taints each unpacked element).
                    if matches!(
                        analysis_data.data_flow_graph.kind,
                        GraphKind::WholeProgram(_)
                    ) {
                        let mapped_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
                        let param = mapped_index
                            .and_then(|mapped| function_info.params.get(mapped))
                            .or_else(|| function_info.params.last().filter(|p| p.is_variadic));
                        if let Some(param) = param {
                            argument_analyzer::add_dataflow(
                                analyzer,
                                &functionlike_id_for_info(function_info),
                                mapped_index.unwrap_or(idx),
                                arg_pos,
                                &arg_type,
                                param,
                                true,
                                context,
                                analysis_data,
                                call_pos,
                            );
                        }
                    }
                }
                continue;
            }

            let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
            let param = param_index
                .and_then(|mapped_index| function_info.params.get(mapped_index))
                .or_else(|| function_info.params.last().filter(|p| p.is_variadic));

            if let (Some(param), Some(arg_type)) =
                (param, get_argument_value_type(analysis_data, arg, arg_pos))
            {
                let effective_param = adjust_param_type(param, template_result);

                argument_analyzer::verify_type(
                    analyzer,
                    arg,
                    arg_pos,
                    &arg_type,
                    &effective_param,
                    param_index.unwrap_or(idx),
                    callable_name,
                    analysis_data,
                    context,
                    Some(call_dataflow_for_info(function_info, call_pos)),
                );
            }
        }
    }

    arg_param_indices
}

fn adjust_param_type(
    param: &pzoom_code_info::functionlike_info::ParamInfo,
    template_result: Option<&TemplateResult>,
) -> pzoom_code_info::functionlike_info::ParamInfo {
    let mut adjusted_param = param.clone();

    let Some(template_result) = template_result else {
        return adjusted_param;
    };

    if crate::template::template_result_is_empty(template_result) {
        return adjusted_param;
    }

    if let Some(param_type) = param.get_type() {
        adjusted_param.param_type = Some(replace_template_param_union(param_type, template_result));
    }

    if let Some(signature_type) = &param.signature_type {
        adjusted_param.signature_type = Some(replace_template_param_union(
            signature_type,
            template_result,
        ));
    }

    if let Some(default_type) = &param.default_type {
        adjusted_param.default_type =
            Some(replace_template_param_union(default_type, template_result));
    }

    adjusted_param
}

fn replace_template_param_union(source_type: &TUnion, template_result: &TemplateResult) -> TUnion {
    let standin_replaced = template::standin_type_replacer::replace(source_type, template_result);

    template::inferred_type_replacer::replace(&standin_replaced, template_result)
}

pub(crate) fn get_argument_value_type<'a>(
    analysis_data: &'a FunctionAnalysisData,
    arg: &Argument<'_>,
    arg_pos: Pos,
) -> Option<std::rc::Rc<TUnion>> {
    analysis_data.expr_types.get(&arg_pos).cloned().or_else(|| {
        let span = arg.value().span();
        analysis_data
            .expr_types
            .get(&(span.start.offset, span.end.offset))
            .cloned()
    })
}

fn resolve_argument_param_indices(
    analyzer: &StatementsAnalyzer<'_>,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    params: &[ParamInfo],
    callable_name: &str,
    no_named_arguments: bool,
    analysis_data: &mut FunctionAnalysisData,
) -> Vec<Option<usize>> {
    let mut result = vec![None; args.len()];
    let mut next_positional_param = 0usize;
    let mut saw_named_argument = false;
    let mut emitted_no_named_arguments_issue = false;
    let mut consumed_non_variadic_params = FxHashSet::default();
    let variadic_param_index = params.iter().rposition(|param| param.is_variadic);

    for (arg_index, arg) in args.iter().enumerate() {
        if arg.is_unpacked() {
            // A spread of a known array shape: string keys act as named
            // arguments (Psalm validates them the same way) and int keys as
            // positionals.
            let arg_pos = arg_positions.get(arg_index).copied().unwrap_or((0, 0));
            if let Some(spread_type) = get_argument_value_type(analysis_data, arg, arg_pos)
                && let [
                    pzoom_code_info::TAtomic::TKeyedArray {
                        properties,
                        fallback_value_type: None,
                        ..
                    },
                ] = spread_type.types.as_slice()
            {
                let mut keys: Vec<&pzoom_code_info::ArrayKey> = properties.keys().collect();
                keys.sort();
                for key in keys {
                    match key {
                        pzoom_code_info::ArrayKey::Int(_) => {
                            if next_positional_param < params.len()
                                && !params[next_positional_param].is_variadic
                            {
                                consumed_non_variadic_params.insert(next_positional_param);
                                next_positional_param += 1;
                            }
                        }
                        pzoom_code_info::ArrayKey::String(name)
                        | pzoom_code_info::ArrayKey::ClassString(name) => {
                            let named_param_index = params
                                .iter()
                                .position(|param| matches_named_argument(analyzer, param, name));
                            match named_param_index {
                                None if variadic_param_index.is_none() => {
                                    add_argument_issue(
                                        analyzer,
                                        analysis_data,
                                        arg_pos,
                                        IssueKind::InvalidNamedArgument,
                                        format!(
                                            "Parameter ${} does not exist on function {}",
                                            name, callable_name
                                        ),
                                    );
                                }
                                Some(param_index)
                                    if consumed_non_variadic_params.contains(&param_index)
                                        && !params[param_index].is_variadic =>
                                {
                                    add_argument_issue(
                                        analyzer,
                                        analysis_data,
                                        arg_pos,
                                        IssueKind::InvalidNamedArgument,
                                        format!(
                                            "Parameter ${} overwrites previous argument to {}",
                                            name, callable_name
                                        ),
                                    );
                                }
                                Some(param_index) => {
                                    if !params[param_index].is_variadic {
                                        consumed_non_variadic_params.insert(param_index);
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
            continue;
        }

        let arg_pos = arg_positions.get(arg_index).copied().unwrap_or((0, 0));

        match arg {
            Argument::Named(named_arg) => {
                saw_named_argument = true;

                if no_named_arguments && !emitted_no_named_arguments_issue {
                    emitted_no_named_arguments_issue = true;
                    add_argument_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::NamedArgumentNotAllowed,
                        format!(
                            "{} is marked with @no-named-arguments and cannot be called with named arguments",
                            callable_name
                        ),
                    );
                }

                let named_param_index = params
                    .iter()
                    .position(|param| matches_named_argument(analyzer, param, named_arg.name.value))
                    .or(variadic_param_index);

                let Some(param_index) = named_param_index else {
                    add_argument_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::InvalidNamedArgument,
                        format!(
                            "Invalid named argument {} passed to {}",
                            named_arg.name.value, callable_name
                        ),
                    );
                    continue;
                };

                let is_variadic = params
                    .get(param_index)
                    .is_some_and(|param| param.is_variadic);
                if !is_variadic && consumed_non_variadic_params.contains(&param_index) {
                    add_argument_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::InvalidNamedArgument,
                        format!(
                            "Parameter {} of {} is already specified",
                            named_arg.name.value, callable_name
                        ),
                    );
                    continue;
                }

                if !is_variadic {
                    consumed_non_variadic_params.insert(param_index);
                }

                result[arg_index] = Some(param_index);
            }
            Argument::Positional(_) => {
                if saw_named_argument {
                    add_argument_issue(
                        analyzer,
                        analysis_data,
                        arg_pos,
                        IssueKind::InvalidNamedArgument,
                        "Cannot use positional argument after named argument".to_string(),
                    );
                    continue;
                }

                let param_index = if next_positional_param < params.len() {
                    Some(next_positional_param)
                } else {
                    variadic_param_index
                };

                if let Some(param_index) = param_index {
                    if !params[param_index].is_variadic {
                        consumed_non_variadic_params.insert(param_index);
                    }
                    result[arg_index] = Some(param_index);
                }

                next_positional_param = next_positional_param.saturating_add(1);
            }
        }
    }

    result
}

pub(crate) fn matches_named_argument(
    analyzer: &StatementsAnalyzer<'_>,
    param: &ParamInfo,
    arg_name: &str,
) -> bool {
    let param_name = analyzer.interner.lookup(param.name);
    param_name
        .as_ref()
        .strip_prefix('$')
        .unwrap_or(param_name.as_ref())
        == arg_name
}

fn add_argument_issue(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    arg_pos: Pos,
    issue_kind: IssueKind,
    message: String,
) {
    let (line, col) = analyzer.get_line_column(arg_pos.0);
    analysis_data.add_issue(Issue::new(
        issue_kind,
        message,
        analyzer.file_path,
        arg_pos.0,
        arg_pos.1,
        line,
        col,
    ));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    func_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_result: &TemplateResult,
    call_pos: Pos,
) {
    let args: Vec<_> = func_call.argument_list.arguments.iter().collect();
    let span = func_call.argument_list.span();
    let arg_param_indices = check_arguments_match(
        analyzer,
        &args,
        arg_positions,
        func_info,
        func_name,
        analysis_data,
        context,
        Some(template_result),
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

    function_call_analyzer::maybe_check_builtin_callable_arity(
        analyzer,
        func_name,
        &args,
        arg_positions,
        analysis_data,
        context,
    );

    let array_filter_callback_param_type = infer_array_filter_callback_param_type_for_validation(
        analyzer,
        func_info.name,
        &args,
        arg_positions,
        analysis_data,
    );

    // Verify each argument type
    for (idx, arg) in args.iter().enumerate() {
        let arg_pos = arg_positions.get(idx).copied().unwrap_or((0, 0));

        if arg.is_unpacked() {
            if let Some(arg_type) = get_argument_value_type(analysis_data, arg, arg_pos) {
                argument_analyzer::verify_unpacked_argument(
                    analyzer,
                    arg_pos,
                    &arg_type,
                    func_name,
                    func_info.no_named_arguments,
                    analysis_data,
                );

                // The unpacked elements still flow into the mapped (or
                // trailing variadic) parameter; verify the element type like
                // a normal argument (Psalm checks each unpacked element).
                let element_param = arg_param_indices
                    .get(idx)
                    .and_then(|mapped| *mapped)
                    .and_then(|mapped_index| func_info.params.get(mapped_index))
                    .or_else(|| func_info.params.last().filter(|param| param.is_variadic));
                // Template-typed params bind from sibling arguments, which
                // the per-element substitution cannot replicate — only verify
                // template-free signatures.
                let mut verified_element = false;
                if crate::template::template_result_is_empty(template_result)
                    && let (Some(param), Some(element_type)) = (
                        element_param,
                        unpacked_iterable_element_type(analyzer.codebase, &arg_type),
                    )
                {
                    // The element type loses the array's dataflow parents, so
                    // taint dataflow is attached separately below.
                    verified_element = !matches!(
                        analysis_data.data_flow_graph.kind,
                        GraphKind::WholeProgram(_)
                    );
                    argument_analyzer::verify_type(
                        analyzer,
                        arg,
                        arg_pos,
                        &element_type,
                        param,
                        arg_param_indices
                            .get(idx)
                            .and_then(|mapped| *mapped)
                            .unwrap_or(idx),
                        func_name,
                        analysis_data,
                        context,
                        verified_element.then(|| call_dataflow_for_info(func_info, call_pos)),
                    );
                }

                if !verified_element
                    && matches!(
                        analysis_data.data_flow_graph.kind,
                        GraphKind::WholeProgram(_)
                    )
                    && let Some(param) = element_param
                {
                    // Whole-program (taint) mode: the unpacked array's values
                    // flow into the parameter (Psalm taints each unpacked
                    // element with the array's own dataflow parents).
                    argument_analyzer::add_dataflow(
                        analyzer,
                        &functionlike_id_for_info(func_info),
                        arg_param_indices
                            .get(idx)
                            .and_then(|mapped| *mapped)
                            .unwrap_or(idx),
                        arg_pos,
                        &arg_type,
                        param,
                        true,
                        context,
                        analysis_data,
                        call_pos,
                    );
                }
            }
            continue;
        }

        let param_index = arg_param_indices.get(idx).and_then(|mapped| *mapped);
        let param = param_index
            .and_then(|mapped_index| func_info.params.get(mapped_index))
            .or_else(|| func_info.params.last().filter(|p| p.is_variadic));

        if let Some(param) = param {
            if let Some(arg_type) = get_argument_value_type(analysis_data, arg, arg_pos) {
                let mut effective_param = param.clone();
                if !crate::template::template_result_is_empty(template_result) {
                    if let Some(param_type) = param.get_type() {
                        effective_param.param_type =
                            Some(function_call_analyzer::replace_templates_in_union(
                                param_type,
                                template_result,
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
                    func_info.name,
                    &args,
                    arg_positions,
                    analysis_data,
                    context,
                    arg,
                    param_index.unwrap_or(idx),
                ) {
                    maybe_relax_array_map_callback_param_for_validation(
                        func_info.name,
                        arg,
                        param_index.unwrap_or(idx),
                        &mut effective_param,
                    );
                }

                argument_analyzer::verify_type(
                    analyzer,
                    arg,
                    arg_pos,
                    &arg_type,
                    &effective_param,
                    param_index.unwrap_or(idx),
                    func_name,
                    analysis_data,
                    context,
                    Some(call_dataflow_for_info(func_info, call_pos)),
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

    let var_id = VarName::new(direct_var.name);

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
        invalidate_property_narrowings_for_argument(context, direct_var.name);
        return;
    }

    if let Some(intersection) =
        assertion_reconciler::intersect_union_with_union(arg_type, param_type)
        && &intersection != arg_type
    {
        context.set_var_type_for_inference(var_id, intersection);
        invalidate_property_narrowings_for_argument(context, direct_var.name);
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
    invalidate_property_narrowings_for_argument(context, direct_var.name);
}

fn invalidate_property_narrowings_for_argument(context: &mut BlockContext, var_name: &str) {
    let prefix = format!("{}->", var_name);
    let keys_to_remove: Vec<_> = context
        .locals
        .keys()
        .filter(|local_id| local_id.starts_with(prefix.as_str()))
        .cloned()
        .collect();

    for local_id in keys_to_remove {
        context.locals.remove(&local_id);
    }
}

pub(crate) fn infer_array_filter_callback_param_type_for_validation(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    infer_array_filter_callback_param_type(
        analyzer,
        function_id,
        args,
        arg_positions,
        analysis_data,
        true,
    )
}

/// Untyped closure params are inferred from the array argument with NO
/// literal/variable gate — Psalm's handleArrayMapFilterArrayArg fills the
/// synthetic ArrayValue template from the arg's node-data type even for
/// call results; only the validation-side params provider is gated
/// (psalm#8905).
pub(crate) fn infer_array_filter_callback_param_type_for_closure_inference(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    infer_array_filter_callback_param_type(
        analyzer,
        function_id,
        args,
        arg_positions,
        analysis_data,
        false,
    )
}

fn infer_array_filter_callback_param_type(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    gate_first_arg: bool,
) -> Option<TUnion> {
    if !is_array_filter_function_name(function_id) {
        return None;
    }

    let callback_pos = *arg_positions.get(1)?;
    let callback_is_null = analysis_data
        .expr_types
        .get(&callback_pos)
        .cloned()
        .is_some_and(|callback_type| callback_type.is_null());
    if callback_is_null {
        return None;
    }

    // Psalm's ArrayFilterParamsProvider only types the callback from literal
    // or variable first args (SimpleTypeInferer + vars_in_scope lookup — not
    // call results, per psalm#8905); anything else falls back to a plain
    // array, leaving the callback param mixed and unchecked.
    let first_arg_supports_inference = !gate_first_arg
        || args.first().is_some_and(|arg| {
            let expr = arg.value().unparenthesized();
            crate::expression_identifier::get_expression_var_key(expr).is_some()
                || matches!(
                    expr,
                    Expression::Array(_)
                        | Expression::LegacyArray(_)
                        | Expression::Literal(_)
                        | Expression::ConstantAccess(_)
                )
        });

    let (value_type, key_type) = if let Some(array_pos) = arg_positions
        .first()
        .copied()
        .filter(|_| first_arg_supports_inference)
    {
        if let Some(array_type) = analysis_data.expr_types.get(&array_pos).cloned() {
            if let Some(array_info) =
                function_call_analyzer::extract_array_like_info_from_union(&array_type)
            {
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
        let raw_mode = infer_array_filter_mode(
            mode_arg,
            analysis_data.expr_types.get(&mode_pos).cloned().as_deref(),
        );

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

    let value_type = function_call_analyzer::widen_literal_scalar_union_for_callable(&value_type);
    let key_type = function_call_analyzer::widen_literal_scalar_union_for_callable(&key_type);

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
        if issue.location.start_offset < first_arg_pos.0
            || issue.location.start_offset > first_arg_pos.1
        {
            return true;
        }

        !matches!(
            issue.kind,
            IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
        )
    });
}

pub(crate) fn is_array_filter_function_name(function_id: StrId) -> bool {
    // Psalm's ArgumentsAnalyzer::ARRAY_FILTERLIKE.
    matches!(
        function_id,
        StrId::ARRAY_FILTER
            | StrId::ARRAY_FIND
            | StrId::ARRAY_FIND_KEY
            | StrId::ARRAY_ANY
            | StrId::ARRAY_ALL
    )
}

pub(crate) fn predeclare_by_ref_argument_vars(
    analyzer: &StatementsAnalyzer<'_>,
    function_name: Option<&str>,
    func_info: Option<&pzoom_code_info::FunctionLikeInfo>,
    args: &mago_syntax::ast::sequence::TokenSeparatedSequence<
        '_,
        mago_syntax::ast::ast::argument::Argument<'_>,
    >,
    context: &mut BlockContext,
) {
    if function_name.is_none() {
        return;
    }

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
            || mapped_param_idx.is_some_and(|param_idx| {
                func_info.is_some_and(|info| is_preg_match_out_param(info.name, param_idx))
            });
        if !treat_as_by_ref {
            continue;
        }

        if let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) =
            arg.value().unparenthesized()
        {
            // Superglobals are always defined (and taint sources) — masking
            // them with a placeholder would swallow `extract($_POST)` taints.
            if crate::expr::variable_fetch_analyzer::is_superglobal(
                direct.name.trim_start_matches('$'),
            ) {
                continue;
            }

            let var_id = VarName::new(direct.name);
            if !context.locals.contains_key(&var_id) {
                // Psalm leaves these variables typeless and skips argument
                // verification for them; mark the placeholder so verify_type
                // can do the same.
                let mut placeholder = TUnion::mixed();
                placeholder.from_undefined_by_ref = true;
                context.set_var_type(var_id, placeholder);
            }
        } else if context.collect_initializations
            && let Some(property_key) =
                crate::expression_identifier::get_expression_var_key(arg.value().unparenthesized())
            && property_key.contains("->")
        {
            // While collecting constructor initialisations, a by-ref property-path
            // argument (`$this->foo($this->bar)`) initialises that property through
            // the reference — predeclare it so reading it as the argument is not
            // mistaken for an uninitialised read. Gated to the collect pass: in
            // normal analysis a property keeps its declared type (so e.g.
            // `array_push($this->ints, …)` still type-checks the element).
            let var_id = VarName::new(&property_key);
            if !context.locals.contains_key(&var_id) {
                let mut placeholder = TUnion::mixed();
                placeholder.from_undefined_by_ref = true;
                context.set_var_type(var_id, placeholder);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn apply_param_out_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    functionlike_template_types: &[pzoom_code_info::functionlike_info::FunctionTemplateType],
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_result: &TemplateResult,
    call_pos: Pos,
) {
    // Bind conditional-subject templates (`$param`, TFunctionArgCount, PHP
    // version tokens) the same way the return-type fetcher does, so
    // conditional @param-out types pick a branch.
    let mut param_arg_types: rustc_hash::FxHashMap<StrId, TUnion> =
        rustc_hash::FxHashMap::default();
    for (index, param_info) in params.iter().enumerate() {
        if let Some(param_arg_pos) = arg_positions.get(index) {
            if let Some(arg_type) = analysis_data.expr_types.get(&*param_arg_pos).cloned() {
                param_arg_types.insert(param_info.name, (*arg_type).clone());
            }
        } else if let Some(default_type) = &param_info.default_type {
            param_arg_types.insert(param_info.name, default_type.clone());
        }
    }
    let mut effective_template_result = template_result.clone();
    super::function_call_return_type_fetcher::inject_subject_template_replacements(
        analyzer,
        functionlike_template_types,
        args.len(),
        &param_arg_types,
        &mut effective_template_result,
    );
    let template_result = &effective_template_result;

    let mut by_ref_var_ids = FxHashSet::default();
    let mut next_positional_param = 0usize;
    for (arg_offset, arg) in args.iter().enumerate() {
        let Some(param_idx) =
            resolve_param_index_for_argument(analyzer, arg, next_positional_param, params)
        else {
            continue;
        };
        if matches!(arg, Argument::Positional(_)) {
            next_positional_param = next_positional_param.saturating_add(1);
        }
        let param = &params[param_idx];

        let treat_as_by_ref = param.by_ref || is_preg_match_out_param(function_id, param_idx);
        if !treat_as_by_ref {
            continue;
        }

        // Psalm special-cases array_shift/array_pop by-ref adjustment
        // (ArrayFunctionArgumentsAnalyzer::handleByRefArrayAdjustment) instead
        // of applying the generic @param-out conditional.
        if param_idx == 0 && (function_id == StrId::ARRAY_SHIFT || function_id == StrId::ARRAY_POP)
        {
            // Psalm keys the adjustment by the extended var id, so property
            // paths (array_shift(\$atomic->extra_types)) drop their narrowed
            // non-empty state too.
            if let Some(var_key) = crate::expression_identifier::get_expression_var_key(arg.value())
            {
                super::array_function_arguments_analyzer::handle_by_ref_array_adjustment(
                    analyzer,
                    context,
                    &var_key,
                    function_id == StrId::ARRAY_SHIFT,
                );
                continue;
            }
        }

        // Psalm special-cases array_push/array_unshift the same way
        // (ArrayFunctionArgumentsAnalyzer::handleAddition) — including on
        // property paths, where the written-back type is checked against the
        // declared property type (Psalm's virtual `$arr[] = $v` assignment).
        if param_idx == 0
            && (function_id == StrId::ARRAY_PUSH || function_id == StrId::ARRAY_UNSHIFT)
        {
            if let Some(var_key) = crate::expression_identifier::get_expression_var_key(arg.value())
            {
                if super::array_function_arguments_analyzer::handle_array_addition(
                    analyzer,
                    context,
                    analysis_data,
                    args,
                    arg_positions,
                    &var_key,
                    function_id == StrId::ARRAY_UNSHIFT,
                ) {
                    continue;
                }
            }
        }

        // Psalm's handleByRefFunctionArg: reset/end/next/prev/ksort leave the
        // by-ref array's type untouched (noops) — no @param-out demotion.
        if param_idx == 0
            && matches!(
                analyzer.interner.lookup(function_id).as_ref(),
                "ksort" | "reset" | "end" | "next" | "prev"
            )
        {
            continue;
        }

        // Psalm's handleByRefFunctionArg sort-family arm: krsort/asort/arsort/
        // natcasesort/natsort keep the argument's own array type — a keyed
        // array generalizes (`getGenericArrayType`), a list loses its order
        // guarantee — instead of demoting to the stub's by-ref param type.
        if param_idx == 0
            && matches!(
                analyzer.interner.lookup(function_id).as_ref(),
                "krsort" | "asort" | "arsort" | "natcasesort" | "natsort"
            )
        {
            if let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) =
                arg.value().unparenthesized()
                && let Some(existing) = context.get_var_type(direct.name)
            {
                let mut array_atomics: Vec<TAtomic> = Vec::new();
                for atomic in &existing.types {
                    match atomic {
                        TAtomic::TKeyedArray {
                            properties,
                            fallback_key_type,
                            fallback_value_type,
                            ..
                        } => {
                            let (key_type, value_type) =
                                crate::expr::array_analyzer::get_keyed_array_generic_params(
                                    properties,
                                    fallback_key_type.as_deref(),
                                    fallback_value_type.as_deref(),
                                );
                            array_atomics.push(TAtomic::TNonEmptyArray {
                                key_type: Box::new(key_type),
                                value_type: Box::new(value_type),
                            });
                        }
                        TAtomic::TList { value_type } => {
                            array_atomics.push(TAtomic::TArray {
                                key_type: Box::new(TUnion::int()),
                                value_type: value_type.clone(),
                            });
                        }
                        TAtomic::TNonEmptyList { value_type } => {
                            array_atomics.push(TAtomic::TNonEmptyArray {
                                key_type: Box::new(TUnion::int()),
                                value_type: value_type.clone(),
                            });
                        }
                        TAtomic::TArray { .. } | TAtomic::TNonEmptyArray { .. } => {
                            array_atomics.push(atomic.clone());
                        }
                        _ => {}
                    }
                }
                if !array_atomics.is_empty() {
                    let mut by_ref_type = TUnion::from_types(array_atomics);
                    by_ref_type.parent_nodes = existing.parent_nodes.clone();
                    context
                        .locals
                        .insert(VarName::new(direct.name), by_ref_type);
                }
            }
            continue;
        }

        // Psalm's ArrayFunctionArgumentsAnalyzer::handleSplice: with a
        // replacement argument, the by-ref array becomes the combination of
        // its (list-ified) own type and the replacement's value type. Psalm
        // keys the write-back by the extended var id, so a property path
        // (array_splice($obj->prop, ...)) is handled the same as a plain
        // variable instead of falling through to the generic @param-out, which
        // would drop the element type to mixed.
        if param_idx == 0 && analyzer.interner.lookup(function_id).as_ref() == "array_splice" {
            if let Some(var_key) = crate::expression_identifier::get_expression_var_key(arg.value())
                && super::array_function_arguments_analyzer::handle_splice_by_ref(
                    context,
                    analysis_data,
                    args,
                    arg_positions,
                    &var_key,
                )
            {
                continue;
            }
        }

        // Conditional `@param-out` types (e.g. preg_match's TFlags-keyed
        // $matches shape) resolve inside the template replacement layer.
        let mut resolved_out_type = if let Some(param_out_type) = &param.param_out_type {
            function_call_analyzer::replace_templates_in_union_in(
                Some(analyzer.codebase),
                param_out_type,
                template_result,
            )
        } else if let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) {
            if crate::template::template_result_is_empty(template_result) {
                param_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(param_type, template_result)
            }
        } else {
            TUnion::mixed()
        };

        // Resolve class-constant references/wildcards (`Foo::BAR_*`) the same
        // way the call-site argument checker does — Psalm expands them via
        // TypeExpander before the write-back.
        let callable_name = analyzer.interner.lookup(function_id);
        resolved_out_type = super::callable_validation::normalize_class_constant_param_type(
            analyzer,
            &resolved_out_type,
            callable_name.as_ref(),
        );

        // Hakana `handle_possibly_matching_inout_param`: the written-back value
        // flows out of the callee through a `FunctionLikeOut` node. PHP by-ref
        // params are the analogue of Hack inout params here.
        let assignment_node = add_by_ref_argument_dataflow(
            analyzer,
            &FunctionLikeIdentifier::Function(function_id),
            function_id,
            param_idx,
            arg_offset,
            args,
            arg_positions,
            analysis_data,
            call_pos,
        );
        resolved_out_type.parent_nodes = vec![assignment_node];

        if let Expression::Variable(mago_syntax::ast::ast::variable::Variable::Direct(direct)) =
            arg.value().unparenthesized()
        {
            let var_id = VarName::new(direct.name);

            // Psalm's $constrain_type: by-ref params of CallMap (builtin)
            // functions don't constrain the variable — `shuffle($a)` leaves
            // $a free to be reassigned to anything.
            let function_is_builtin = analyzer
                .codebase
                .get_function(function_id)
                .and_then(|function_info| analyzer.codebase.files.get(&function_info.file_path))
                .is_some_and(|file_info| file_info.is_stub);
            if let Some(constraint_type) = param.get_type().or(param.signature_type.as_ref())
                && !function_is_builtin
                && !constraint_type.is_mixed()
                && var_id != "$this"
            {
                let constraint_type =
                    super::callable_validation::normalize_class_constant_param_type(
                        analyzer,
                        constraint_type,
                        callable_name.as_ref(),
                    );
                context.add_reference_constraint(var_id.clone(), constraint_type);
            }

            if var_id.as_str() != "$this" {
                context.set_var_type(var_id.clone(), resolved_out_type);
                by_ref_var_ids.insert(var_id.clone());
            }
        } else if let Some(property_key) =
            crate::expression_identifier::get_expression_var_key(arg.value().unparenthesized())
            && property_key.contains("->")
        {
            // A by-ref `@param-out` argument that is a property path
            // (`$obj->prop`) assigns that property through the reference — Psalm
            // writes `vars_in_scope[$obj->prop]` with the out-type. This also lets
            // a constructor initialise a property by passing it to a `@param-out`
            // helper (`$this->foo($this->bar)`).
            let var_id = VarName::new(&property_key);
            context.set_var_type(var_id.clone(), resolved_out_type);
            by_ref_var_ids.insert(var_id);
        }
    }

    if !by_ref_var_ids.is_empty() {
        context.clauses =
            BlockContext::remove_reconciled_clause_refs(&context.clauses, &by_ref_var_ids).0;
    }
}

/// Port of the dataflow portion of Hakana
/// `arguments_analyzer::handle_possibly_matching_inout_param` (Hack inout ≈ PHP
/// by-ref): build a `FunctionLikeOut` node for the written-back argument, flow
/// the argument's previous value into it (function-body graphs), and add the
/// special `preg_match`/`preg_match_all` pattern/subject edges (Hakana models
/// these on `preg_match_with_matches`).
///
/// Conservative deviations:
/// - the out-node's `arg_location` uses the argument expression position
///   (pzoom's `ParamInfo` has no `name_location`);
/// - the call is always treated as specialized (Hakana specializes nearly all
///   calls);
/// - comment-based removed taints (`HAKANA_SECURITY_IGNORE`) are not ported
///   (whole-program taint mode only);
/// - Hack-only `json_decode_with_error` handling is skipped (PHP `json_decode`
///   has no by-ref error param).
#[allow(clippy::too_many_arguments)]
fn add_by_ref_argument_dataflow(
    analyzer: &StatementsAnalyzer<'_>,
    functionlike_id: &FunctionLikeIdentifier,
    function_id: StrId,
    param_idx: usize,
    arg_offset: usize,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
    call_pos: Pos,
) -> DataFlowNode {
    let arg_pos = arg_positions.get(arg_offset).copied().unwrap_or(call_pos);
    let call_node_pos = make_data_flow_node_position(analyzer, call_pos);

    let assignment_node = DataFlowNode::get_for_method_argument_out(
        functionlike_id,
        param_idx,
        Some(make_data_flow_node_position(analyzer, arg_pos)),
        Some(call_node_pos),
    );

    if let GraphKind::FunctionBody = analysis_data.data_flow_graph.kind {
        let parent_nodes = analysis_data
            .expr_types
            .get(&arg_pos)
            .cloned()
            .map(|arg_type| arg_type.parent_nodes.clone())
            .unwrap_or_default();

        for arg_node in &parent_nodes {
            analysis_data.data_flow_graph.add_path(
                &arg_node.id,
                &assignment_node.id,
                PathKind::Default,
                vec![],
                vec![],
            );
        }
    }

    if is_preg_match_out_param(function_id, param_idx) && args.len() >= 2 {
        let subject_pos = make_data_flow_node_position(
            analyzer,
            arg_positions.get(1).copied().unwrap_or(call_pos),
        );

        let argument_node = DataFlowNode::get_for_method_argument(
            functionlike_id,
            0,
            Some(subject_pos),
            Some(call_node_pos),
        );

        analysis_data
            .data_flow_graph
            .add_node(argument_node.clone());

        analysis_data.data_flow_graph.add_path(
            &argument_node.id,
            &assignment_node.id,
            PathKind::Aggregate,
            vec![],
            vec![],
        );

        let argument_node = DataFlowNode::get_for_method_argument(
            functionlike_id,
            1,
            Some(subject_pos),
            Some(call_node_pos),
        );

        analysis_data
            .data_flow_graph
            .add_node(argument_node.clone());

        analysis_data.data_flow_graph.add_path(
            &argument_node.id,
            &assignment_node.id,
            PathKind::Default,
            vec![],
            vec![],
        );
    }

    analysis_data
        .data_flow_graph
        .add_node(assignment_node.clone());

    assignment_node
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

fn is_preg_match_out_param(function_id: StrId, param_idx: usize) -> bool {
    if param_idx != 2 {
        return false;
    }

    matches!(function_id, StrId::PREG_MATCH | StrId::PREG_MATCH_ALL)
}

/// Collect direct variables that a call assigns through by-ref parameters
/// (including the `preg_match` out-param), e.g. `foo($x)` with `&$param`.
pub(crate) fn collect_call_by_ref_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
) -> FxHashSet<VarName> {
    let mut assigned = FxHashSet::default();

    let Call::Function(function_call) = call else {
        return assigned;
    };

    let Expression::Identifier(function_identifier) = function_call.function.unparenthesized()
    else {
        return assigned;
    };

    let raw_name = function_identifier.value();
    let resolved_name_id = analyzer
        .get_resolved_name(function_identifier.start_offset() as u32)
        .unwrap_or_else(|| analyzer.interner.intern(raw_name));
    let function_info = analyzer
        .codebase
        .get_function(resolved_name_id)
        .or_else(|| {
            analyzer
                .codebase
                .get_function(analyzer.interner.intern(raw_name))
        });

    for (idx, arg) in function_call.argument_list.arguments.iter().enumerate() {
        let Expression::Variable(Variable::Direct(direct)) = arg.value().unparenthesized() else {
            continue;
        };

        let by_ref_from_signature = function_info.and_then(|info| {
            if idx < info.params.len() {
                Some(info.params[idx].by_ref)
            } else {
                info.params
                    .last()
                    .filter(|param| param.is_variadic)
                    .map(|p| p.by_ref)
            }
        });

        let treat_as_by_ref = by_ref_from_signature.unwrap_or(false)
            || function_info.is_some_and(|info| is_preg_match_out_param(info.name, idx));
        if treat_as_by_ref {
            assigned.insert(VarName::new(direct.name));
        }
    }

    assigned
}

fn should_relax_array_map_callback_validation(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &FunctionAnalysisData,
    context: &BlockContext,
    arg: &Argument<'_>,
    param_index: usize,
) -> bool {
    if !function_call_analyzer::is_array_map_function_name(function_id) || param_index != 0 {
        return false;
    }

    if matches!(
        arg.value().unparenthesized(),
        Expression::Closure(_) | Expression::ArrowFunction(_)
    ) {
        return false;
    }

    function_call_analyzer::callback_argument_is_pure(
        analyzer,
        args,
        arg_positions,
        analysis_data,
        context,
        0,
    )
}

fn maybe_relax_array_map_callback_param_for_validation(
    function_id: StrId,
    arg: &Argument<'_>,
    param_index: usize,
    effective_param: &mut ParamInfo,
) {
    if param_index != 0 || !function_call_analyzer::is_array_map_function_name(function_id) {
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

/// Psalm's `ArrayFunctionArgumentsAnalyzer::handleByRefArrayAdjustment`: the
/// written-back type of `array_shift($x)` / `array_pop($x)` is derived from
/// `$x`'s current type per atomic, not from the stub's generic
/// `@param-out` conditional.
/// The element (value) type produced by unpacking an iterable argument, or
/// `None` when no member is a known iterable.
pub(crate) fn unpacked_element_type_for_templates(
    codebase: &pzoom_code_info::CodebaseInfo,
    arg_type: &TUnion,
) -> Option<TUnion> {
    unpacked_iterable_element_type(codebase, arg_type)
}

fn unpacked_iterable_element_type(
    codebase: &pzoom_code_info::CodebaseInfo,
    arg_type: &TUnion,
) -> Option<TUnion> {
    let mut element_type: Option<TUnion> = None;
    for atomic in &arg_type.types {
        let member = match atomic {
            TAtomic::TArray { value_type, .. }
            | TAtomic::TNonEmptyArray { value_type, .. }
            | TAtomic::TIterable { value_type, .. }
            | TAtomic::TList { value_type }
            | TAtomic::TNonEmptyList { value_type } => Some((**value_type).clone()),
            // Unpacking a Traversable object (`f(...$generator)`) contributes
            // its value param, remapped through @template-extends when the
            // object is a Traversable subtype.
            TAtomic::TNamedObject {
                name,
                type_params: Some(type_params),
                ..
            } => {
                let mapped =
                    crate::type_comparator::object_type_comparator::get_mapped_generic_type_params(
                        codebase,
                        *name,
                        type_params,
                        pzoom_str::StrId::TRAVERSABLE,
                    )
                    .unwrap_or_else(|| type_params.clone());
                mapped.get(1).cloned()
            }
            TAtomic::TKeyedArray {
                properties,
                fallback_value_type,
                ..
            } => {
                let mut combined = fallback_value_type.as_deref().cloned();
                for property_type in properties.values() {
                    combined = Some(match combined {
                        Some(existing) => {
                            pzoom_code_info::combine_union_types(&existing, property_type, false)
                        }
                        None => property_type.clone(),
                    });
                }
                combined
            }
            _ => None,
        };
        if let Some(member) = member {
            element_type = Some(match element_type {
                Some(existing) => pzoom_code_info::combine_union_types(&existing, &member, false),
                None => member,
            });
        }
    }
    element_type
}
