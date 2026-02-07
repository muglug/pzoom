//! Multiple arguments analyzer.
//!
//! This module analyzes all arguments in a function/method call.
//! Argument type verification against function parameters is handled
//! by the individual call analyzers (function_call_analyzer, method_call_analyzer, etc.)
//! which have access to the function signature.

use mago_span::HasSpan;
use mago_syntax::ast::ast::argument::{Argument, ArgumentList};
use pzoom_code_info::functionlike_info::ParamInfo;
use pzoom_code_info::{FunctionLikeInfo, Issue, IssueKind, TUnion};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::BlockContext;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::template;

use super::{argument_analyzer, callable_validation};

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
    template_defaults: Option<&FxHashMap<StrId, TUnion>>,
    template_replacements: Option<&FxHashMap<StrId, TUnion>>,
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
                    callable_validation::verify_unpacked_argument(
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

                callable_validation::verify_argument_type(
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
    template_defaults: Option<&FxHashMap<StrId, TUnion>>,
    template_replacements: Option<&FxHashMap<StrId, TUnion>>,
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
    template_replacements: &FxHashMap<StrId, TUnion>,
    template_defaults: &FxHashMap<StrId, TUnion>,
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
