//! Loop fixpoint analyzer.
//!
//! This is a port of Hakana's `loop_analyzer` (which itself mirrors Psalm's
//! `LoopAnalyzer`). It analyzes a loop body repeatedly until the inferred types of
//! the variables it assigns stabilise, so that types widened by the loop (e.g. a
//! counter that grows, or a variable conditionally reassigned) are accounted for.
//!
//! Adaptations for pzoom:
//! * Locals are stored as `TUnion` (not `Rc<TUnion>`) keyed by interned `StrId`.
//! * Condition formulae come from [`formula_generator::get_formula`]; the post-loop
//!   negation uses [`negate_formula`] exactly as Hakana does.
//! * The active [`LoopScope`] is threaded via `analysis_data.loop_scopes` (a stack)
//!   rather than an explicit parameter, so `break`/`continue` can update it.

use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::statement::Statement;
use rustc_hash::{FxHashMap, FxHashSet};

use pzoom_code_info::algebra::{Clause, get_truths_from_formula, negate_formula, simplify_cnf};
use pzoom_code_info::combine_union_types;
use pzoom_str::StrId;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{BreakContext, ControlAction};
use crate::stmt::loop_::{assignment_map_visitor::get_assignment_map, tast_cleaner::clean_nodes};
use crate::stmt_analyzer;

/// Analyze a loop body to a fixed point.
///
/// Returns the updated [`LoopScope`] and the resulting inner loop context (the
/// `continue` context, or for `do` loops the context after the body but before the
/// final condition).
#[allow(clippy::too_many_arguments)]
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    stmts: &[Statement<'_>],
    pre_conditions: Vec<&Expression<'_>>,
    post_expressions: Vec<&Expression<'_>>,
    mut loop_scope: LoopScope,
    loop_context: &mut BlockContext,
    loop_parent_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    is_do: bool,
    always_enters_loop: bool,
) -> Result<(LoopScope, BlockContext), AnalysisError> {
    // Baseline assignment counts before the loop, so we can later invalidate clauses
    // for any variable the loop body (re)assigns — even when the reassignment leaves
    // the top-level type unchanged, e.g. `unset($a['k'][$i])` which rewrites `$a` to
    // the same type. Mirrors Hakana invalidating clauses for loop-redefined vars.
    let pre_loop_assigned_counts = loop_parent_context.assigned_var_ids.clone();

    let (assignment_map, first_var_id) = get_assignment_map(&pre_conditions, &post_expressions, stmts);

    let assignment_depth = if let Some(first_var_id) = first_var_id {
        get_assignment_map_depth(&first_var_id, &mut assignment_map.clone())
    } else {
        0
    };

    let mut always_assigned_before_loop_body_vars: FxHashSet<StrId> = FxHashSet::default();

    // Build the CNF formula for each pre-condition up front (mirrors Hakana).
    let mut pre_condition_clauses: Vec<Vec<Clause>> = Vec::new();
    if !pre_conditions.is_empty() {
        for pre_condition in &pre_conditions {
            let pre_condition_id = (
                pre_condition.start_offset() as u32,
                pre_condition.end_offset() as u32,
            );
            pre_condition_clauses.push(
                formula_generator::get_formula(
                    pre_condition_id,
                    pre_condition_id,
                    pre_condition,
                    analyzer,
                    analysis_data,
                    false,
                )
                .unwrap_or_default(),
            );
        }
    } else {
        always_assigned_before_loop_body_vars =
            BlockContext::get_new_or_updated_locals(loop_parent_context, loop_context);
    }

    // Determine whether the body unconditionally breaks (its only control action is Break).
    let mut break_context = loop_context.break_types.clone();
    break_context.push(BreakContext::Loop);
    let final_actions =
        crate::stmt::scope_analyzer::get_control_actions(stmts, analysis_data, &break_context, true);
    let does_always_break =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Break);

    let mut continue_context;
    let mut inner_do_context: Option<BlockContext> = None;

    if assignment_depth == 0 || does_always_break {
        continue_context = loop_context.clone();

        for (offset, pre_condition) in pre_conditions.iter().enumerate() {
            apply_pre_condition_to_loop_context(
                analyzer,
                pre_condition,
                &pre_condition_clauses[offset],
                &mut continue_context,
                loop_parent_context,
                analysis_data,
                is_do,
            );
        }

        analyze_loop_body(analyzer, stmts, analysis_data, &mut continue_context, &mut loop_scope)?;
        update_loop_scope_contexts(&loop_scope, loop_context, &mut continue_context, loop_parent_context);

        loop_context.inside_loop_exprs = true;
        for post_expression in &post_expressions {
            let _ = expression_analyzer::analyze(analyzer, post_expression, analysis_data, loop_context);
        }
        loop_context.inside_loop_exprs = false;
    } else {
        let original_parent_context = loop_parent_context.clone();
        let mut pre_loop_context = loop_context.clone();

        analysis_data.start_recording_issues();

        if !is_do {
            for (offset, pre_condition) in pre_conditions.iter().enumerate() {
                apply_pre_condition_to_loop_context(
                    analyzer,
                    pre_condition,
                    &pre_condition_clauses[offset],
                    loop_context,
                    loop_parent_context,
                    analysis_data,
                    is_do,
                );
            }
        }

        continue_context = loop_context.clone();

        analyze_loop_body(analyzer, stmts, analysis_data, &mut continue_context, &mut loop_scope)?;
        update_loop_scope_contexts(
            &loop_scope,
            loop_context,
            &mut continue_context,
            &original_parent_context,
        );

        if is_do {
            inner_do_context = Some(continue_context.clone());

            for (offset, pre_condition) in pre_conditions.iter().enumerate() {
                always_assigned_before_loop_body_vars.extend(apply_pre_condition_to_loop_context(
                    analyzer,
                    pre_condition,
                    &pre_condition_clauses[offset],
                    &mut continue_context,
                    loop_parent_context,
                    analysis_data,
                    is_do,
                ));
            }
        }

        continue_context.inside_loop_exprs = true;
        for post_expression in &post_expressions {
            let _ =
                expression_analyzer::analyze(analyzer, post_expression, analysis_data, &mut continue_context);
        }
        continue_context.inside_loop_exprs = false;

        let mut recorded_issues = analysis_data.clear_currently_recorded_issues();
        analysis_data.stop_recording_issues();

        let mut i = 0;
        while i < assignment_depth {
            loop_scope.iteration_count += 1;

            let mut has_changes = false;
            let mut vars_to_remove = Vec::new();

            if pre_loop_context
                .locals
                .keys()
                .any(|var_id| !continue_context.locals.contains_key(var_id))
            {
                has_changes = true;
            }

            let mut different_from_pre_loop_types: FxHashSet<StrId> = FxHashSet::default();

            for (var_id, continue_context_type) in continue_context.locals.clone() {
                if always_assigned_before_loop_body_vars.contains(&var_id) {
                    if let Some(pre_loop_context_type) = pre_loop_context.locals.get(&var_id) {
                        if continue_context_type != *pre_loop_context_type {
                            different_from_pre_loop_types.insert(var_id);
                            has_changes = true;
                        }
                    } else {
                        has_changes = true;
                    }
                } else if let Some(parent_context_type) = original_parent_context.locals.get(&var_id) {
                    if continue_context_type != *parent_context_type {
                        has_changes = true;

                        let widened =
                            combine_union_types(&continue_context_type, parent_context_type, false);
                        continue_context.locals.insert(var_id, widened);

                        pre_loop_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
                        loop_parent_context.possibly_assigned_var_ids.insert(var_id);
                    }

                    if let Some(loop_context_type) = loop_context.locals.get(&var_id) {
                        if continue_context_type != *loop_context_type {
                            has_changes = true;
                        }
                        let widened =
                            combine_union_types(&continue_context_type, loop_context_type, false);
                        continue_context.locals.insert(var_id, widened);
                        pre_loop_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
                    }
                } else {
                    if !recorded_issues.is_empty() {
                        has_changes = true;
                    }
                    if !is_do {
                        vars_to_remove.push(var_id);
                    }
                }
            }

            continue_context.has_returned = false;

            if !has_changes {
                break;
            }

            for var_id in vars_to_remove {
                continue_context.locals.remove(&var_id);
            }

            continue_context.clauses.clone_from(&pre_loop_context.clauses);

            analysis_data.start_recording_issues();

            if !is_do {
                for (offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        analyzer,
                        pre_condition,
                        &pre_condition_clauses[offset],
                        &mut continue_context,
                        loop_parent_context,
                        analysis_data,
                        is_do,
                    );
                }
            }

            for var_id in &always_assigned_before_loop_body_vars {
                let pre_loop_context_type = pre_loop_context.locals.get(var_id);

                let should_reset = if different_from_pre_loop_types.contains(var_id) {
                    true
                } else if continue_context.locals.contains_key(var_id) {
                    pre_loop_context_type.is_none()
                } else {
                    true
                };

                if should_reset {
                    if let Some(pre_loop_context_type) = pre_loop_context_type {
                        continue_context
                            .locals
                            .insert(*var_id, pre_loop_context_type.clone());
                    } else {
                        continue_context.locals.remove(var_id);
                    }
                }
            }

            continue_context.clauses.clone_from(&pre_loop_context.clauses);

            clean_nodes(stmts, analysis_data);

            analyze_loop_body(analyzer, stmts, analysis_data, &mut continue_context, &mut loop_scope)?;
            update_loop_scope_contexts(
                &loop_scope,
                loop_context,
                &mut continue_context,
                &original_parent_context,
            );

            if is_do {
                inner_do_context = Some(continue_context.clone());

                for (offset, pre_condition) in pre_conditions.iter().enumerate() {
                    apply_pre_condition_to_loop_context(
                        analyzer,
                        pre_condition,
                        &pre_condition_clauses[offset],
                        &mut continue_context,
                        loop_parent_context,
                        analysis_data,
                        is_do,
                    );
                }
            }

            continue_context.inside_loop_exprs = true;
            for post_expression in &post_expressions {
                let _ = expression_analyzer::analyze(
                    analyzer,
                    post_expression,
                    analysis_data,
                    &mut continue_context,
                );
            }
            continue_context.inside_loop_exprs = false;

            recorded_issues = analysis_data.clear_currently_recorded_issues();
            analysis_data.stop_recording_issues();

            i += 1;
        }

        for recorded_issue in recorded_issues {
            analysis_data.bubble_up_issue(recorded_issue);
        }
    }

    let does_sometimes_break = loop_scope.final_actions.contains(&ControlAction::Break);
    let does_always_break = does_sometimes_break && loop_scope.final_actions.len() == 1;

    if does_sometimes_break {
        if let Some(inner_do_context_inner) = inner_do_context.as_mut() {
            for (var_id, possibly_redefined_var_type) in &loop_scope.possibly_redefined_loop_parent_vars
            {
                if let Some(do_context_type) = inner_do_context_inner.locals.get_mut(var_id) {
                    *do_context_type = if do_context_type == possibly_redefined_var_type {
                        possibly_redefined_var_type.clone()
                    } else {
                        combine_union_types(possibly_redefined_var_type, do_context_type, false)
                    };
                }
                loop_parent_context.possibly_assigned_var_ids.insert(*var_id);
            }
        } else {
            for (var_id, var_type) in &loop_scope.possibly_redefined_loop_parent_vars {
                if let Some(loop_parent_context_type) = loop_parent_context.locals.get_mut(var_id) {
                    *loop_parent_context_type =
                        combine_union_types(var_type, loop_parent_context_type, false);
                }
                loop_parent_context.possibly_assigned_var_ids.insert(*var_id);
            }
        }
    }

    for (var_id, var_type) in loop_parent_context.locals.clone() {
        if let Some(loop_context_type) = loop_context.locals.get(&var_id) {
            if *loop_context_type != var_type {
                let combined = combine_union_types(&var_type, loop_context_type, false);
                loop_parent_context.locals.insert(var_id, combined);
                loop_parent_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
            }
        }
    }

    // Invalidate clauses for any variable the loop body (re)assigned, even when its
    // top-level type is unchanged. Without this, a clause like `$a['foo']` truthy
    // established before the loop would wrongly survive a body that does
    // `unset($a['foo'][$i])`, producing spurious paradox/redundancy diagnostics.
    for (var_id, body_count) in &continue_context.assigned_var_ids {
        let pre_count = pre_loop_assigned_counts.get(var_id).copied().unwrap_or(0);
        if *body_count > pre_count {
            loop_parent_context.remove_var_from_conflicting_clauses(*var_id, analyzer.interner);
        }
    }

    if !does_always_break {
        for (var_id, var_type) in loop_parent_context.locals.clone() {
            if let Some(continue_context_type) = continue_context.locals.get(&var_id) {
                if continue_context_type.is_mixed() {
                    loop_parent_context
                        .locals
                        .insert(var_id, continue_context_type.clone());
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
                } else if *continue_context_type != var_type {
                    let combined = combine_union_types(&var_type, continue_context_type, false);
                    loop_parent_context.locals.insert(var_id, combined);
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
                }
            } else {
                loop_parent_context.locals.remove(&var_id);
            }
        }
    }

    // If the loop contains a condition and there are no break statements, we can
    // negate that condition and apply it to the post-loop context (mirrors Hakana).
    if !pre_conditions.is_empty() && !pre_condition_clauses.is_empty() && !does_sometimes_break {
        let negated_pre_condition_clauses =
            negate_formula(pre_condition_clauses.into_iter().flatten().collect()).unwrap_or_default();

        let (negated_pre_condition_types, _) = get_truths_from_formula(
            negated_pre_condition_clauses.iter().collect(),
            None,
            &mut FxHashSet::default(),
        );

        if !negated_pre_condition_types.is_empty() {
            let mut changed_var_ids = FxHashSet::default();
            reconciler::reconcile_keyed_types(
                &negated_pre_condition_types,
                &mut continue_context,
                &mut changed_var_ids,
                analyzer,
                analysis_data,
                true,
                false,
                false,
                None,
            );

            for var_id in changed_var_ids {
                if let Some(reconciled_type) = continue_context.locals.get(&var_id) {
                    if loop_parent_context.locals.contains_key(&var_id) {
                        loop_parent_context
                            .locals
                            .insert(var_id, reconciled_type.clone());
                    }
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id, analyzer.interner);
                }
            }
        }
    }

    if always_enters_loop {
        let does_sometimes_continue = loop_scope.final_actions.contains(&ControlAction::Continue);

        for (var_id, var_type) in &continue_context.locals {
            if does_sometimes_break || does_sometimes_continue {
                if let Some(possibly_defined_type) =
                    loop_scope.possibly_defined_loop_parent_vars.get(var_id)
                {
                    loop_parent_context.locals.insert(
                        *var_id,
                        combine_union_types(var_type, possibly_defined_type, false),
                    );
                }
            } else {
                loop_parent_context.locals.insert(*var_id, var_type.clone());
            }
        }
    }

    // Variables a leaving branch inside the body could have defined become
    // possibly-in-scope after the loop (Psalm folds LoopScope::vars_possibly_in_scope
    // back into the surrounding context).
    for var_id in &loop_scope.vars_possibly_in_scope {
        loop_parent_context.vars_possibly_in_scope.insert(*var_id);
        if !loop_parent_context.locals.contains_key(var_id) {
            loop_parent_context.possibly_assigned_var_ids.insert(*var_id);
        }
    }

    // Propagate references created inside the loop body so the parent scope knows
    // they originated in a confusing (loop) scope and are unsafe to reuse.
    loop_parent_context.update_references_possibly_from_confusing_scope(&continue_context);

    let inner_context = inner_do_context.unwrap_or_else(|| loop_context.clone());
    Ok((loop_scope, inner_context))
}

/// Push the loop scope, analyze the body (so `break`/`continue` can update it), then
/// pop it back.
fn analyze_loop_body(
    analyzer: &StatementsAnalyzer<'_>,
    stmts: &[Statement<'_>],
    analysis_data: &mut FunctionAnalysisData,
    continue_context: &mut BlockContext,
    loop_scope: &mut LoopScope,
) -> Result<(), AnalysisError> {
    analysis_data.loop_scopes.push(loop_scope.clone());
    let result = stmt_analyzer::analyze_stmts(analyzer, stmts, analysis_data, continue_context);
    if let Some(updated) = analysis_data.loop_scopes.pop() {
        *loop_scope = updated;
    }
    result
}

fn get_assignment_map_depth(
    first_var_id: &str,
    assignment_map: &mut FxHashMap<String, FxHashSet<String>>,
) -> usize {
    let mut max_depth = 0;

    let Some(assignment_var_ids) = assignment_map.remove(first_var_id) else {
        return 0;
    };

    for assignment_var_id in assignment_var_ids {
        let mut depth = 1;

        if assignment_map.contains_key(&assignment_var_id) {
            depth += get_assignment_map_depth(&assignment_var_id, assignment_map);
        }

        if depth > max_depth {
            max_depth = depth;
        }
    }

    max_depth
}

/// Analyze a pre-condition inside the loop context, apply its formula's narrowing.
/// Returns the set of variables (re)assigned before the loop body relative to the
/// parent context.
fn apply_pre_condition_to_loop_context(
    analyzer: &StatementsAnalyzer<'_>,
    pre_condition: &Expression<'_>,
    pre_condition_clauses: &[Clause],
    loop_context: &mut BlockContext,
    loop_parent_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    is_do: bool,
) -> FxHashSet<StrId> {
    let pre_referenced_var_ids = loop_context.cond_referenced_var_ids.clone();
    loop_context.cond_referenced_var_ids = FxHashSet::default();

    loop_context.inside_conditional = true;
    loop_context.inside_loop_exprs = true;

    let _ = expression_analyzer::analyze(analyzer, pre_condition, analysis_data, loop_context);

    loop_context.inside_loop_exprs = false;
    loop_context.inside_conditional = false;

    loop_context
        .cond_referenced_var_ids
        .extend(pre_referenced_var_ids);
    let mut new_referenced_var_ids: FxHashSet<String> = FxHashSet::default();

    let always_assigned_before_loop_body_vars =
        BlockContext::get_new_or_updated_locals(loop_context, loop_parent_context);

    // loop_context.clauses = simplify_cnf(parent clauses + pre-condition clauses)
    let mut combined_clause_refs: Vec<&Clause> =
        loop_context_parent_clause_refs(loop_parent_context);
    combined_clause_refs.extend(pre_condition_clauses.iter());
    loop_context.clauses = simplify_cnf(combined_clause_refs)
        .into_iter()
        .map(Rc::new)
        .collect();

    let cond_id = (
        pre_condition.start_offset() as u32,
        pre_condition.end_offset() as u32,
    );
    let clause_refs: Vec<&Clause> = loop_context.clauses.iter().map(|c| c.as_ref()).collect();
    let (reconcilable_while_types, active_while_types) =
        get_truths_from_formula(clause_refs, Some(cond_id), &mut new_referenced_var_ids);

    if !reconcilable_while_types.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &reconcilable_while_types,
            loop_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            true,
            false,
            false,
            Some(&active_while_types),
        );
    }

    if is_do {
        return FxHashSet::default();
    }

    always_assigned_before_loop_body_vars
}

fn loop_context_parent_clause_refs(loop_parent_context: &BlockContext) -> Vec<&Clause> {
    loop_parent_context
        .clauses
        .iter()
        .map(|c| c.as_ref())
        .collect()
}

fn update_loop_scope_contexts(
    loop_scope: &LoopScope,
    loop_context: &mut BlockContext,
    continue_context: &mut BlockContext,
    pre_outer_context: &BlockContext,
) {
    if !loop_scope.final_actions.contains(&ControlAction::Continue) {
        loop_context.locals = pre_outer_context.locals.clone();
    } else {
        for (var_id, var_type) in &loop_scope.possibly_redefined_loop_vars {
            if continue_context.has_variable(*var_id) {
                let combined = combine_union_types(
                    continue_context.locals.get(var_id).unwrap(),
                    var_type,
                    false,
                );
                continue_context.locals.insert(*var_id, combined);
            }
        }
    }
}
