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

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, get_truths_from_formula, negate_formula, simplify_cnf};
use pzoom_code_info::combine_union_types;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::LoopScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::loop_::{assignment_map_visitor::get_assignment_map, tast_cleaner::clean_nodes};
use crate::stmt::scope_analyzer::ControlAction;
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
    while_true: bool,
) -> Result<(LoopScope, BlockContext), AnalysisError> {
    let (assignment_map, first_var_id) =
        get_assignment_map(&pre_conditions, &post_expressions, stmts);

    let assignment_depth = if let Some(first_var_id) = first_var_id {
        get_assignment_map_depth(&first_var_id, &mut assignment_map.clone())
    } else {
        0
    };

    let mut always_assigned_before_loop_body_vars: FxHashSet<VarName> = FxHashSet::default();

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

    // Determine whether the body unconditionally breaks (its only control
    // action is Break). Psalm's LoopAnalyzer computes this with EMPTY break
    // types: the body's own break/continue surface as Break/Continue, while
    // nested loops consume theirs.
    let final_actions =
        crate::stmt::scope_analyzer::get_control_actions(stmts, analysis_data, &[], true);
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

        analyze_loop_body(
            analyzer,
            stmts,
            analysis_data,
            &mut continue_context,
            &mut loop_scope,
        )?;
        update_loop_scope_contexts(
            &loop_scope,
            loop_context,
            &mut continue_context,
            loop_parent_context,
        );

        loop_context.inside_loop_exprs = true;
        for post_expression in &post_expressions {
            let _ = expression_analyzer::analyze(
                analyzer,
                post_expression,
                analysis_data,
                loop_context,
            );
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

        analyze_loop_body(
            analyzer,
            stmts,
            analysis_data,
            &mut continue_context,
            &mut loop_scope,
        )?;
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
            let _ = expression_analyzer::analyze(
                analyzer,
                post_expression,
                analysis_data,
                &mut continue_context,
            );
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

            let mut different_from_pre_loop_types: FxHashSet<VarName> = FxHashSet::default();

            for (var_id, continue_context_type) in continue_context.locals.clone() {
                if always_assigned_before_loop_body_vars.contains(&var_id) {
                    if let Some(pre_loop_context_type) = pre_loop_context.locals.get(&var_id) {
                        if continue_context_type != *pre_loop_context_type {
                            if !loop_types_equal(&continue_context_type, pre_loop_context_type) {
                                different_from_pre_loop_types.insert(var_id);
                            }
                            has_changes = true;
                        }
                    } else {
                        has_changes = true;
                    }
                } else if let Some(parent_context_type) =
                    original_parent_context.locals.get(&var_id)
                {
                    // `has_changes` (whether another pass runs) keeps the
                    // metadata-sensitive comparison — dataflow parents changed
                    // by the body need a re-pass so loop-carried reads connect
                    // to the new assignment nodes. The WIDENING and CLAUSE
                    // PURGING use Psalm's `Union::equals` semantics (types and
                    // flags only): a parent-scoped var whose type merely
                    // re-ordered or grew new parent nodes must not lose
                    // inherited disjunction clauses (`$calling || $file`).
                    if continue_context_type != *parent_context_type {
                        has_changes = true;
                    }
                    if !loop_types_equal(&continue_context_type, parent_context_type) {
                        let widened =
                            combine_union_types(&continue_context_type, parent_context_type, false);
                        continue_context.locals.insert(var_id.clone(), widened);

                        pre_loop_context.remove_var_from_conflicting_clauses(var_id.clone());
                        // NOTE: the variable already existed in the parent
                        // scope — a loop-body reassignment widens its type but
                        // cannot make it possibly-undefined. It IS possibly
                        // assigned, though (Psalm's LoopAnalyzer marks the
                        // loop parent) — an enclosing if's merge needs that
                        // to keep the var possibly-redefined.
                        loop_parent_context
                            .possibly_assigned_var_ids
                            .insert(var_id.clone());
                    }

                    if let Some(loop_context_type) = loop_context.locals.get(&var_id) {
                        if continue_context_type != *loop_context_type {
                            has_changes = true;
                        }
                        if !loop_types_equal(&continue_context_type, loop_context_type) {
                            let widened = combine_union_types(
                                &continue_context_type,
                                loop_context_type,
                                false,
                            );
                            continue_context.locals.insert(var_id.clone(), widened);
                            pre_loop_context.remove_var_from_conflicting_clauses(var_id.clone());
                        }
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

            continue_context
                .clauses
                .clone_from(&pre_loop_context.clauses);
            // Psalm's LoopAnalyzer resets byref constraints to the pre-loop
            // state for each reanalysis pass — a constraint established by a
            // by-ref call inside the body must not flag the loop-top
            // reassignment of the same var.
            continue_context
                .reference_constraints
                .clone_from(&pre_loop_context.reference_constraints);

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
                            .insert(var_id.clone(), pre_loop_context_type.clone());
                    } else {
                        continue_context.locals.remove(var_id);
                    }
                }
            }

            continue_context
                .clauses
                .clone_from(&pre_loop_context.clauses);

            clean_nodes(stmts, analysis_data);

            analyze_loop_body(
                analyzer,
                stmts,
                analysis_data,
                &mut continue_context,
                &mut loop_scope,
            )?;
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
        // Psalm merges break-time variable states straight into the parent
        // context (modern LoopAnalyzer has no inner-do special case here).
        for (var_id, var_type) in &loop_scope.possibly_redefined_loop_parent_vars {
            if let Some(loop_parent_context_type) = loop_parent_context.locals.get_mut(var_id) {
                *loop_parent_context_type =
                    combine_union_types(var_type, loop_parent_context_type, false);
            }
            loop_parent_context
                .possibly_assigned_var_ids
                .insert(var_id.clone());
        }
    }

    for (var_id, var_type) in loop_parent_context.locals.clone() {
        if let Some(loop_context_type) = loop_context.locals.get(&var_id) {
            if *loop_context_type != var_type {
                let combined = combine_union_types(&var_type, loop_context_type, false);
                loop_parent_context.locals.insert(var_id.clone(), combined);
                loop_parent_context.remove_var_from_conflicting_clauses(var_id.clone());
            }
        }
    }

    if !does_always_break {
        for (var_id, var_type) in loop_parent_context.locals.clone() {
            if let Some(continue_context_type) = continue_context.locals.get_mut(&var_id) {
                if continue_context_type.is_mixed() {
                    // Mixed widening must not sever the pre-loop assignment's
                    // flow to post-loop uses: merge the parent's dataflow
                    // parents into the continue-context value *in place* (Psalm
                    // reassigns `$continue_context->vars_in_scope`, Hakana mutates
                    // via `get_mut`) so a later `setLoopVars` copy keeps them.
                    for parent_node in &var_type.parent_nodes {
                        if !continue_context_type.parent_nodes.contains(parent_node) {
                            continue_context_type.parent_nodes.push(parent_node.clone());
                        }
                    }
                    loop_parent_context
                        .locals
                        .insert(var_id.clone(), continue_context_type.clone());
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id.clone());
                } else if *continue_context_type != var_type {
                    let combined = combine_union_types(&var_type, continue_context_type, false);
                    loop_parent_context.locals.insert(var_id.clone(), combined);
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id.clone());
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
            negate_formula(pre_condition_clauses.into_iter().flatten().collect())
                .unwrap_or_default();

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
                crate::reconciler::EmissionMode::Silent,
                None,
            );

            for var_id in changed_var_ids {
                if let Some(reconciled_type) = continue_context.locals.get(&var_id) {
                    if loop_parent_context.locals.contains_key(&var_id) {
                        loop_parent_context
                            .locals
                            .insert(var_id.clone(), reconciled_type.clone());
                    }
                    loop_parent_context.remove_var_from_conflicting_clauses(var_id.clone());
                }
            }
        }
    }

    // Psalm copies loop vars only when the loop can actually exit
    // (`$always_enters_loop && $can_leave_loop`): a `while (true)` with no
    // break never reaches the code after it, so its assignments stay inside.
    let can_leave_loop = !while_true || does_sometimes_break;

    if always_enters_loop && can_leave_loop {
        let does_sometimes_continue = loop_scope.final_actions.contains(&ControlAction::Continue);

        // Variables assigned before the loop body (e.g. a foreach key/value)
        // that are never reassigned inside the body are guaranteed to hold
        // their loop-assigned type after the loop, even if it breaks or
        // continues, so they must not be merged back to their pre-loop type
        // (Psalm's setLoopVars $always_assigned_before_loop_body_vars).
        let always_assigned_unmodified_vars: FxHashSet<&VarName> =
            always_assigned_before_loop_body_vars
                .iter()
                .filter(|var_id| !assignment_map.contains_key(var_id.as_ref()))
                .collect();

        for (var_id, var_type) in &continue_context.locals {
            if (does_sometimes_break || does_sometimes_continue)
                && !always_assigned_unmodified_vars.contains(var_id)
            {
                if let Some(possibly_defined_type) =
                    loop_scope.possibly_defined_loop_parent_vars.get(var_id)
                {
                    loop_parent_context.locals.insert(
                        var_id.clone(),
                        combine_union_types(var_type, possibly_defined_type, false),
                    );
                }
            } else {
                loop_parent_context
                    .locals
                    .insert(var_id.clone(), var_type.clone());
            }
        }
    }

    // Anything possibly in scope at the body's end is possibly in scope after
    // the loop (Psalm: `$loop_parent_context->vars_possibly_in_scope +=
    // $continue_context->vars_possibly_in_scope`).
    for var_id in &continue_context.vars_possibly_in_scope {
        loop_parent_context
            .vars_possibly_in_scope
            .insert(var_id.clone());
    }

    // Variables a leaving branch inside the body could have defined become
    // possibly-in-scope after the loop (Psalm folds LoopScope::vars_possibly_in_scope
    // back into the surrounding context).
    for var_id in &loop_scope.vars_possibly_in_scope {
        loop_parent_context
            .vars_possibly_in_scope
            .insert(var_id.clone());
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
) -> FxHashSet<VarName> {
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
    let mut new_referenced_var_ids: FxHashSet<VarName> = FxHashSet::default();

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
            crate::reconciler::EmissionMode::Silent,
            Some(&active_while_types),
        );
    }

    if is_do {
        return FxHashSet::default();
    }

    // Strip clauses mentioning any variable assigned before the loop body, so
    // stale truthiness facts about a loop-reassigned variable do not leak into
    // the body (Psalm `Context::filterClauses`, Hakana `BlockContext::filter_clauses`,
    // both called with no new type — a pure removal).
    if !loop_context.clauses.is_empty() {
        for var_id in &always_assigned_before_loop_body_vars {
            loop_context.remove_var_name_clauses(var_id.as_ref());
        }
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
            if continue_context.has_variable(var_id) {
                let combined = combine_union_types(
                    continue_context.locals.get(var_id).unwrap(),
                    var_type,
                    false,
                );
                continue_context.locals.insert(var_id.clone(), combined);
            }
        }
    }
}

/// Loop change-detection equality: Psalm's `Union::equals` compares types and
/// semantic flags but not dataflow parent nodes, which churn every pass.
fn loop_types_equal(a: &pzoom_code_info::TUnion, b: &pzoom_code_info::TUnion) -> bool {
    a.types.len() == b.types.len()
        && a.types.iter().all(|atomic| b.types.contains(atomic))
        && a.from_docblock == b.from_docblock
        && a.from_calculation == b.from_calculation
        && a.possibly_undefined_from_try == b.possibly_undefined_from_try
        && a.ignore_nullable_issues == b.ignore_nullable_issues
        && a.ignore_falsable_issues == b.ignore_falsable_issues
}
