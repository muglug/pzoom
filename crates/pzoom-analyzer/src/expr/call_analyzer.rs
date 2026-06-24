//! Function and method call analyzer.
//!
//! This module dispatches to specialized analyzers in the `call/` submodule,
//! and hosts `reconcile_lower_bounds_with_upper_bounds` (Hakana keeps it in
//! its `expr/call_analyzer.rs` too).

use itertools::Itertools;
use mago_syntax::cst::cst::call::Call;
use pzoom_code_info::code_location::CodeLocation;
use pzoom_code_info::ttype::template::get_relevant_bounds;
use pzoom_code_info::{Issue, IssueKind, TemplateBound};

use crate::context::BlockContext;
use crate::expr::call::{function_call_analyzer, method_call_analyzer, static_call_analyzer};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use crate::type_comparator::type_comparison_result::TypeComparisonResult;
use crate::type_comparator::union_type_comparator;

/// Analyze a function or method call expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match call {
        Call::Function(func_call) => {
            function_call_analyzer::analyze(analyzer, func_call, pos, analysis_data, context);
        }
        Call::Method(method_call) => {
            method_call_analyzer::analyze(analyzer, method_call, pos, analysis_data, context);
        }
        Call::NullSafeMethod(null_safe_call) => {
            method_call_analyzer::analyze_nullsafe(
                analyzer,
                null_safe_call,
                pos,
                analysis_data,
                context,
            );
        }
        Call::StaticMethod(static_call) => {
            static_call_analyzer::analyze(analyzer, static_call, pos, analysis_data, context);
        }
    }
}

/// Verbatim port of Hakana's `reconcile_lower_bounds_with_upper_bounds`
/// (`analyzer/expr/call_analyzer.rs`): reconciles the lower/upper/equality
/// bounds accumulated for a type variable, raising
/// `IncompatibleTypeParameters` when they cannot hold simultaneously.
///
/// Valid constraints:
///
///   T <: int|float, T >: int --- implies T is an int
///   T = int --- implies T is an int
///
/// Invalid constraints:
///
///   T <: int|string, T >: string|float --- implies T <: int and T >: float,
///   which is impossible
///   T = int, T = string --- implies T is a string _and_ an int, which is
///   impossible
fn reconcile_lower_bounds_with_upper_bounds(
    lower_bounds: &[TemplateBound],
    upper_bounds: &[TemplateBound],
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: CodeLocation,
) {
    let codebase = analyzer.codebase;
    let interner = analyzer.interner;

    let relevant_lower_bounds = get_relevant_bounds(lower_bounds);

    let mut has_issue = false;

    let add_issue_at =
        |analysis_data: &mut FunctionAnalysisData, location: CodeLocation, message: String| {
            analysis_data.add_issue(Issue::new(
                IssueKind::IncompatibleTypeParameters,
                message,
                location.file_path,
                location.start_offset,
                location.end_offset,
                location.start_line,
                location.start_column,
            ));
        };

    for relevant_lower_bound in &relevant_lower_bounds {
        for upper_bound in upper_bounds {
            let mut union_comparison_result = TypeComparisonResult::new();

            if !union_type_comparator::is_contained_by(
                codebase,
                &relevant_lower_bound.bound_type,
                &upper_bound.bound_type,
                false,
                false,
                &mut union_comparison_result,
            ) {
                if union_comparison_result.type_coerced_from_mixed == Some(true) {
                    // a bound inferred through mixed gets the same loose gate
                    // Psalm applies when binding templates from mixed
                    // arguments (Psalm be7afcf, TypeVariableTracker)
                    continue;
                }

                has_issue = true;
                add_issue_at(
                    analysis_data,
                    relevant_lower_bound
                        .pos
                        .unwrap_or(upper_bound.pos.unwrap_or(pos)),
                    format!(
                        "Type {} should be a subtype of {}",
                        relevant_lower_bound.bound_type.get_id(Some(interner)),
                        upper_bound.bound_type.get_id(Some(interner))
                    ),
                );
            }
        }
    }

    if !has_issue && relevant_lower_bounds.len() > 1 {
        let bounds_with_equality = lower_bounds
            .iter()
            .filter(|bound| bound.equality_bound_classlike.is_some())
            .collect::<Vec<_>>();

        if bounds_with_equality.is_empty() {
            return;
        }

        let equality_strings = bounds_with_equality
            .iter()
            .map(|bound| bound.bound_type.get_id(Some(interner)))
            .unique()
            .collect::<Vec<_>>();

        if equality_strings.len() > 1 {
            has_issue = true;
            add_issue_at(
                analysis_data,
                bounds_with_equality[0].pos.unwrap_or(pos),
                format!(
                    "Incompatible types found for {} (must have only one of {})",
                    "type variable",
                    equality_strings.join(", "),
                ),
            );
        } else {
            'outer: for lower_bound in lower_bounds {
                if lower_bound.equality_bound_classlike.is_none() {
                    for bound_with_equality in &bounds_with_equality {
                        if union_type_comparator::is_contained_by(
                            codebase,
                            &lower_bound.bound_type,
                            &bound_with_equality.bound_type,
                            false,
                            false,
                            &mut TypeComparisonResult::new(),
                        ) {
                            continue 'outer;
                        }
                    }

                    has_issue = true;
                    add_issue_at(
                        analysis_data,
                        pos,
                        format!(
                            "Incompatible types found for {} ({} is not in {})",
                            "type variable",
                            lower_bound.bound_type.get_id(Some(interner)),
                            equality_strings.join(", "),
                        ),
                    );
                }
            }
        }
    }

    if !has_issue && upper_bounds.len() > 1 {
        let upper_bounds_with_equality = upper_bounds
            .iter()
            .filter(|bound| bound.equality_bound_classlike.is_some())
            .enumerate()
            .collect::<Vec<_>>();

        if upper_bounds_with_equality.is_empty() {
            return;
        }

        for (i, upper_bound_with_equality) in upper_bounds_with_equality {
            for (j, upper_bound) in upper_bounds.iter().enumerate() {
                if i == j {
                    continue;
                }

                if !union_type_comparator::can_expression_types_be_identical(
                    codebase,
                    &upper_bound_with_equality.bound_type,
                    &upper_bound.bound_type,
                ) {
                    add_issue_at(
                        analysis_data,
                        pos,
                        format!(
                            "Incompatible types found for {} ({} is not in {})",
                            "type variable",
                            upper_bound.bound_type.get_id(Some(interner)),
                            upper_bound_with_equality.bound_type.get_id(Some(interner)),
                        ),
                    );
                }
            }
        }
    }
}

/// Hakana's end-of-functionlike type-variable pass
/// (`functionlike_analyzer.rs`): for a top-level function-like, reconcile
/// every accumulated bound set, then clear the map so the next function-like
/// starts fresh. pzoom shares one `FunctionAnalysisData` across nested
/// function-likes, so closure bounds are already merged into the enclosing
/// function's map (Hakana's clone-in/filtered-merge-out collapses to a no-op)
/// and only the outermost function-like reconciles.
pub(crate) fn check_type_variable_bounds_at_function_end(
    analyzer: &StatementsAnalyzer<'_>,
    analysis_data: &mut FunctionAnalysisData,
    pos: CodeLocation,
) {
    let type_bounds = std::mem::take(&mut analysis_data.type_variable_bounds);
    for (name, bounds) in &type_bounds {
        if std::env::var("PZOOM_TPLVAR_DEBUG").is_ok() {
            eprintln!(
                "TPLVAR {} lower=[{}] upper=[{}]",
                name,
                bounds
                    .lower_bounds
                    .iter()
                    .map(|bound| bound.bound_type.get_id(Some(analyzer.interner)))
                    .collect::<Vec<_>>()
                    .join(", "),
                bounds
                    .upper_bounds
                    .iter()
                    .map(|bound| bound.bound_type.get_id(Some(analyzer.interner)))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
    }
    for (_, bounds) in type_bounds {
        reconcile_lower_bounds_with_upper_bounds(
            &bounds.lower_bounds,
            &bounds.upper_bounds,
            analyzer,
            analysis_data,
            pos,
        );
    }
}
