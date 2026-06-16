//! Switch statement analyzer.

use mago_syntax::ast::ast::control_flow::switch::{Switch, SwitchBody};
use pzoom_code_info::{TUnion, VarName};
use rustc_hash::FxHashSet;

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
    let switch_expr_pos = {
        // The switch subject is consumed by the dispatch (general use).
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;
        let pos = expression_analyzer::analyze(analyzer, switch.expression, analysis_data, context);
        context.inside_general_use = was_inside_general_use;
        pos
    };
    let switch_expr_type = analysis_data
        .expr_types
        .get(&switch_expr_pos)
        .cloned()
        .map(|t| (*t).clone())
        .unwrap_or_else(TUnion::mixed);

    // A switch whose subject is a variable holding a dependent `get_class($x)` /
    // `gettype($x)` result (e.g. `$t = gettype($x); switch ($t)`) narrows the
    // *original* `$x`. The dependent type carries `$x`'s id; fall back to it when
    // the subject is not the syntactic `get_class(...)`/`gettype(...)` call.
    let class_string_origin = get_switch_class_string_origin(switch.expression)
        .or_else(|| switch_dependent_class_var(&switch_expr_type));
    let gettype_origin = get_switch_gettype_origin(switch.expression)
        .or_else(|| switch_dependent_type_var(&switch_expr_type).map(|var_id| (var_id, false)));
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
    // Open a frame for break-capture (see merge below).
    analysis_data.switch_break_contexts.push(Vec::new());

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
            gettype_origin.clone(),
            class_string_origin.clone(),
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
    // Contexts captured at `break`s that left this switch join the merge
    // (Hakana merges case_scope.break_vars into the outgoing vars).
    merge_sources.extend(
        analysis_data
            .switch_break_contexts
            .pop()
            .unwrap_or_default(),
    );
    if !all_options_matched {
        merge_sources.push(original_context);
    }

    if !merge_sources.is_empty() {
        // Psalm's switch merge: a variable defined in only some leaving
        // paths is NOT in vars_in_scope afterwards — it survives in
        // vars_possibly_in_scope, and reading it reports
        // PossiblyUndefinedVariable from there (a kept one-branch type would
        // both hide that report and leak into later code).
        let mut defined_in_all: FxHashSet<VarName> =
            merge_sources[0].locals.keys().cloned().collect();
        for branch_context in merge_sources.iter().skip(1) {
            defined_in_all.retain(|var_id| branch_context.locals.contains_key(var_id));
        }

        let mut merged_context = merge_sources.remove(0);
        for branch_context in &merge_sources {
            merged_context.merge(branch_context);
        }

        let one_path_only: Vec<VarName> = merged_context
            .locals
            .keys()
            .filter(|var_id| !defined_in_all.contains(*var_id))
            .cloned()
            .collect();
        for var_id in one_path_only {
            merged_context.locals.remove(&var_id);
            merged_context.vars_possibly_in_scope.insert(var_id.clone());
            merged_context.possibly_assigned_var_ids.insert(var_id);
        }

        *context = merged_context;
    }

    context.has_returned = scope.all_options_returned && all_options_matched;

    Ok(())
}
