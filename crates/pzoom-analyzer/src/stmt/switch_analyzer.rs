//! Switch statement analyzer.

use mago_syntax::ast::ast::control_flow::switch::{Switch, SwitchBody};
use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::scope::SwitchScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaseExitType {
    ReturnThrow,
    Continue,
    Break,
    Hybrid,
}

/// Analyze a switch statement.

use super::switch_case_analyzer::*;

/// Analyze a switch statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    switch: &Switch<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let switch_expr_pos =
        expression_analyzer::analyze(analyzer, switch.expression, analysis_data, context);
    let switch_expr_type = analysis_data
        .get_expr_type(switch_expr_pos)
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // A switch whose subject is a variable holding a dependent `get_class($x)` /
    // `gettype($x)` result (e.g. `$t = gettype($x); switch ($t)`) narrows the
    // *original* `$x`. The dependent type carries `$x`'s id; fall back to it when
    // the subject is not the syntactic `get_class(...)`/`gettype(...)` call.
    let class_string_origin = get_switch_class_string_origin(analyzer, switch.expression)
        .or_else(|| switch_dependent_class_var(&switch_expr_type));
    let gettype_origin = get_switch_gettype_origin(analyzer, switch.expression)
        .or_else(|| switch_dependent_type_var(&switch_expr_type));
    let switch_is_get_class = is_get_class_call(switch.expression);
    let switch_is_true = is_true_literal(switch.expression);

    let cases = match &switch.body {
        SwitchBody::BraceDelimited(body) => body.cases.as_slice(),
        SwitchBody::ColonDelimited(body) => body.cases.as_slice(),
    };

    let original_context = context.clone();

    let mut can_track_remaining = union_all_literals(&switch_expr_type);
    if switch_is_true || switch_is_get_class || gettype_origin.is_some() {
        can_track_remaining = false;
    }
    let original_switch_type = switch_expr_type.clone();

    let case_flow = get_case_exit_flow(cases, analysis_data);

    // Cross-case accumulator state (Psalm/Hakana SwitchScope).
    let mut scope = SwitchScope::new(switch_expr_type.clone());

    for (case_index, case) in cases.iter().enumerate() {
        let (case_actions, case_exit_type) = &case_flow[case_index];

        super::switch_case_analyzer::analyze(
            analyzer,
            case,
            switch.expression,
            case_actions,
            *case_exit_type,
            &switch_expr_type,
            &original_switch_type,
            switch_is_true,
            gettype_origin,
            class_string_origin,
            can_track_remaining,
            context.inside_loop,
            case_index + 1 == cases.len(),
            &original_context,
            &mut scope,
            analysis_data,
        )?;
    }

    let all_options_matched =
        scope.has_default || (can_track_remaining && scope.remaining_switch_type.is_nothing());
    let mut merge_sources = scope.continuing_contexts;
    if !all_options_matched {
        merge_sources.push(original_context);
    }

    if !merge_sources.is_empty() {
        let mut merged_context = merge_sources.remove(0);
        for branch_context in &merge_sources {
            merged_context.merge(branch_context);
        }
        *context = merged_context;
    }

    context.has_returned = scope.all_options_returned && all_options_matched;

    Ok(())
}
