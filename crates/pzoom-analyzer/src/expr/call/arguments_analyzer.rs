//! Multiple arguments analyzer.
//!
//! This module analyzes all arguments in a function/method call.
//! Argument type verification against function parameters is handled
//! by the individual call analyzers (function_call_analyzer, method_call_analyzer, etc.)
//! which have access to the function signature.

use super::function_call_analyzer;

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::{Argument, ArgumentList};
use mago_syntax::ast::ast::call::FunctionCall;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::{
    FunctionLikeInfo, FunctionLikeParameter, Issue, IssueKind, TAtomic, TUnion, combine_union_types,
};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;
use crate::template;

use super::{argument_analyzer, callable_validation};
use crate::template::TemplateMap;

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
    template_defaults: Option<&TemplateMap>,
    template_replacements: Option<&TemplateMap>,
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
                let effective_param =
                    adjust_param_type(param, template_defaults, template_replacements);

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
                );
            }
        }
    }

    arg_param_indices
}

pub(crate) fn adjust_param_type(
    param: &pzoom_code_info::functionlike_info::ParamInfo,
    template_defaults: Option<&TemplateMap>,
    template_replacements: Option<&TemplateMap>,
) -> pzoom_code_info::functionlike_info::ParamInfo {
    let mut adjusted_param = param.clone();

    let (Some(template_defaults), Some(template_replacements)) =
        (template_defaults, template_replacements)
    else {
        return adjusted_param;
    };

    if template_defaults.is_empty() && template_replacements.is_empty() {
        return adjusted_param;
    }

    if let Some(param_type) = param.get_type() {
        adjusted_param.param_type = Some(replace_template_param_union(
            param_type,
            template_replacements,
            template_defaults,
        ));
    }

    if let Some(signature_type) = &param.signature_type {
        adjusted_param.signature_type = Some(replace_template_param_union(
            signature_type,
            template_replacements,
            template_defaults,
        ));
    }

    if let Some(default_type) = &param.default_type {
        adjusted_param.default_type = Some(replace_template_param_union(
            default_type,
            template_replacements,
            template_defaults,
        ));
    }

    adjusted_param
}

fn replace_template_param_union(
    source_type: &TUnion,
    template_replacements: &TemplateMap,
    template_defaults: &TemplateMap,
) -> TUnion {
    let standin_replaced = template::standin_type_replacer::replace(
        source_type,
        template_replacements,
        template_defaults,
    );

    template::inferred_type_replacer::replace(
        &standin_replaced,
        template_replacements,
        template_defaults,
    )
}

pub(crate) fn get_argument_value_type<'a>(
    analysis_data: &'a FunctionAnalysisData,
    arg: &Argument<'_>,
    arg_pos: Pos,
) -> Option<std::rc::Rc<TUnion>> {
    analysis_data.get_expr_type(arg_pos).or_else(|| {
        let span = arg.value().span();
        analysis_data.get_expr_type((span.start.offset, span.end.offset))
    })
}

pub(crate) fn resolve_argument_param_indices(
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

fn matches_named_argument(
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

pub(crate) fn verify_arguments(
    analyzer: &StatementsAnalyzer<'_>,
    func_call: &FunctionCall<'_>,
    arg_positions: &[Pos],
    func_info: &pzoom_code_info::FunctionLikeInfo,
    func_name: &str,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
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
            if let Some(arg_type) =
                get_argument_value_type(analysis_data, arg, arg_pos)
            {
                argument_analyzer::verify_unpacked_argument(
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
                get_argument_value_type(analysis_data, arg, arg_pos)
            {
                let mut effective_param = param.clone();
                if !template_defaults.is_empty() || !template_replacements.is_empty() {
                    if let Some(param_type) = param.get_type() {
                        effective_param.param_type = Some(function_call_analyzer::replace_templates_in_union(
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

                normalize_param_class_casing(analyzer, &mut effective_param);

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

pub(crate) fn maybe_generalize_argument_type_after_call(
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

pub(crate) fn invalidate_property_narrowings_for_argument(
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

pub(crate) fn infer_array_filter_callback_param_type_for_validation(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&Argument<'_>],
    arg_positions: &[Pos],
    analysis_data: &mut FunctionAnalysisData,
) -> Option<TUnion> {
    if !is_array_filter_function_name(function_id) {
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
            if let Some(array_info) = function_call_analyzer::extract_array_like_info_from_union(&array_type) {
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

pub(crate) fn infer_array_filter_mode(mode_arg: &Argument<'_>, mode_type: Option<&TUnion>) -> Option<i64> {
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

pub(crate) fn get_single_literal_int_from_union(union: &TUnion) -> Option<i64> {
    if !union.is_single() {
        return None;
    }

    match union.get_single() {
        Some(TAtomic::TLiteralInt { value }) => Some(*value),
        _ => None,
    }
}

pub(crate) fn suppress_undefined_array_filter_input_issues(
    analysis_data: &mut FunctionAnalysisData,
    first_arg_pos: Pos,
) {
    analysis_data.issues.retain(|issue| {
        if issue.location.start_offset < first_arg_pos.0 || issue.location.start_offset > first_arg_pos.1 {
            return true;
        }

        !matches!(
            issue.kind,
            IssueKind::UndefinedVariable | IssueKind::UndefinedGlobalVariable
        )
    });
}

pub(crate) fn is_array_filter_function_name(function_id: StrId) -> bool {
    function_id == StrId::ARRAY_FILTER
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
            let var_id = analyzer.interner.intern(direct.name);
            if !context.locals.contains_key(&var_id) {
                context.set_var_type(var_id, TUnion::mixed());
            }
        }
    }
}

pub(crate) fn apply_param_out_types(
    analyzer: &StatementsAnalyzer<'_>,
    function_id: StrId,
    args: &[&mago_syntax::ast::ast::argument::Argument<'_>],
    _arg_positions: &[Pos],
    params: &[pzoom_code_info::functionlike_info::ParamInfo],
    context: &mut BlockContext,
    template_defaults: &TemplateMap,
    template_replacements: &TemplateMap,
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

        let treat_as_by_ref = param.by_ref || is_preg_match_out_param(function_id, param_idx);
        if !treat_as_by_ref {
            continue;
        }

        let mut resolved_out_type = if let Some(param_out_type) = &param.param_out_type {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_out_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(param_out_type, template_replacements, template_defaults)
            }
        } else if let Some(param_type) = param.get_type().or(param.signature_type.as_ref()) {
            if template_defaults.is_empty() && template_replacements.is_empty() {
                param_type.clone()
            } else {
                function_call_analyzer::replace_templates_in_union(param_type, template_replacements, template_defaults)
            }
        } else {
            TUnion::mixed()
        };

        if is_preg_match_out_param(function_id, param_idx) {
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

pub(crate) fn resolve_param_index_for_argument(
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

pub(crate) fn is_preg_match_out_param(function_id: StrId, param_idx: usize) -> bool {
    if param_idx != 2 {
        return false;
    }

    matches!(function_id, StrId::PREG_MATCH | StrId::PREG_MATCH_ALL)
}

pub(crate) fn should_relax_array_map_callback_validation(
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

pub(crate) fn maybe_relax_array_map_callback_param_for_validation(
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

pub(crate) fn normalize_param_class_casing(
    analyzer: &StatementsAnalyzer<'_>,
    param: &mut ParamInfo,
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
        TAtomic::TNamedObject {
            name, type_params, ..
        } => {
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
