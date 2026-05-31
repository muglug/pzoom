//! Else clause analysis.
//!
//! Faithful port of Psalm's `Block/IfElse/ElseAnalyzer::analyze`. The `else` body
//! is analysed in the final fallthrough context, with the accumulated negated
//! clauses (from the `if` and every `elseif` condition) simplified into the
//! context before reconciliation. Branch results are folded back into the shared
//! [`IfScope`] via [`update_if_scope`], exactly as Psalm folds them through
//! `IfAnalyzer::updateIfScope`.
//!
//! Pzoom's [`BlockContext`] does not carry Psalm's `vars_possibly_in_scope`,
//! `loop_scope`, `collect_exceptions` or `parent_remove_vars`, so those branches
//! are approximated (or omitted) and called out inline.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_syntax::ast::ast::statement::Statement;
use rustc_hash::{FxHashMap, FxHashSet};

use pzoom_code_info::algebra::{Clause, get_truths_from_formula, simplify_cnf};

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::IfScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_else_analyzer::update_if_scope;
use crate::stmt::scope_analyzer::{self, ControlAction};
use crate::stmt_analyzer::analyze_stmts;

/// Mirrors `ElseAnalyzer::analyze`. Mutates `if_scope`, `else_context` and
/// `outer_context` in place.
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    else_stmts: Option<&[Statement<'_>]>,
    if_scope: &mut IfScope,
    else_context: &mut BlockContext,
    outer_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    // No `else`, nothing to negate and no surviving clauses: there is no else
    // branch to reason about. Seed empty fold state so the merge tail treats the
    // missing else as "fallthrough always possible".
    if else_stmts.is_none() && if_scope.negated_clauses.is_empty() && else_context.clauses.is_empty()
    {
        let mut final_actions = FxHashSet::default();
        final_actions.insert(ControlAction::None);
        final_actions.extend(if_scope.final_actions.iter().copied());
        if_scope.final_actions = final_actions;

        if_scope.assigned_var_ids = Some(FxHashMap::default());
        if_scope.new_vars = Some(BTreeMap::new());
        if_scope.redefined_vars = Some(FxHashMap::default());
        if_scope.reasonable_clauses = vec![];

        return Ok(());
    }

    // else_context->clauses = simplifyCNF(else_context->clauses + negated_clauses)
    let mut combined_clauses: Vec<Clause> =
        else_context.clauses.iter().map(|c| (**c).clone()).collect();
    combined_clauses.extend(if_scope.negated_clauses.iter().cloned());
    let simplified = simplify_cnf(combined_clauses.iter().collect());
    else_context.clauses = simplified.into_iter().map(Rc::new).collect();

    let mut cond_referenced_var_ids = FxHashSet::default();
    let (else_types, _active_else_types) = get_truths_from_formula(
        else_context.clauses.iter().map(|c| c.as_ref()).collect(),
        None,
        &mut cond_referenced_var_ids,
    );

    // Psalm clones the pre-reconciliation else_context and feeds it to
    // updateIfScope as the comparison baseline (so reconciled-but-unassigned
    // narrowings don't count as redefinitions).
    let original_context = else_context.clone();

    if !else_types.is_empty() {
        let inside_loop = else_context.inside_loop;
        let mut changed_var_ids = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &else_types,
            else_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            false,
            None,
        );

        if !changed_var_ids.is_empty() {
            else_context.clauses = BlockContext::remove_reconciled_clause_refs(
                &else_context.clauses,
                &changed_var_ids,
                analyzer.interner,
            )
            .0;
            // Psalm additionally drops possible references onto array/property
            // offsets of the changed vars; pzoom's reference model is coarser and
            // is handled by update_references_possibly_from_confusing_scope below.
        }
    }

    // The else context after negated narrowing but before the body — Psalm's
    // `$old_else_context`, the baseline for `Context::update`.
    let old_else_context = else_context.clone();

    let pre_stmts_assigned_var_ids = std::mem::take(&mut else_context.assigned_var_ids);
    let pre_possibly_assigned_var_ids = std::mem::take(&mut else_context.possibly_assigned_var_ids);

    if let Some(else_stmts) = else_stmts {
        analyze_stmts(analyzer, else_stmts, analysis_data, else_context)?;
    }

    // new_assigned_var_ids = vars assigned by the else body; restore the
    // pre-existing assignment counts underneath them (Psalm's `+=`).
    let new_assigned_var_ids = else_context.assigned_var_ids.clone();
    for (var_id, count) in pre_stmts_assigned_var_ids {
        else_context.assigned_var_ids.entry(var_id).or_insert(count);
    }
    let new_possibly_assigned_var_ids = else_context.possibly_assigned_var_ids.clone();
    else_context
        .possibly_assigned_var_ids
        .extend(pre_possibly_assigned_var_ids);

    // Carry by-ref constraints discovered in the else into the outer context so a
    // later conflicting binding is still caught (Psalm's byref_constraints merge;
    // pzoom does not emit ConflictingReferenceConstraint here).
    for (var_id, constraints) in &else_context.reference_constraints {
        let entry = outer_context.reference_constraints.entry(*var_id).or_default();
        for constraint in constraints {
            if !entry.contains(constraint) {
                entry.push(constraint.clone());
            }
        }
    }

    let final_actions = match else_stmts {
        Some(else_stmts) => scope_analyzer::get_control_actions(else_stmts, analysis_data, &[], true),
        None => {
            let mut actions = FxHashSet::default();
            actions.insert(ControlAction::None);
            actions
        }
    };

    let has_ending_statements =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::End);
    let has_leaving_statements = has_ending_statements
        || (!final_actions.is_empty() && !final_actions.contains(&ControlAction::None));
    let has_break_statement =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Break);
    let has_continue_statement =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Continue);

    if_scope.final_actions.extend(final_actions.iter().copied());

    // If the else doesn't leave, fold its redefinitions into the if_scope.
    if !has_leaving_statements {
        let if_cond_changed_var_ids = if_scope.if_cond_changed_var_ids.clone();
        update_if_scope(
            analyzer,
            if_scope,
            else_context,
            &original_context,
            &new_assigned_var_ids,
            &new_possibly_assigned_var_ids,
            &if_cond_changed_var_ids,
            true,
        );

        if_scope.reasonable_clauses = vec![];
    }

    // Propagate the else branch's narrowing of the (negated) condition variables
    // back into the outer context — Psalm's `$outer_context->update(...)`.
    if !if_scope.negatable_if_types.is_empty() {
        let negatable = if_scope.negatable_if_types.clone();
        let mut updated_vars = std::mem::take(&mut if_scope.updated_vars);
        outer_context.update(
            &old_else_context,
            else_context,
            has_leaving_statements,
            &negatable,
            &mut updated_vars,
        );
        if_scope.updated_vars = updated_vars;
    }

    if !has_ending_statements {
        let vars_possibly_in_scope: FxHashSet<_> = else_context
            .vars_possibly_in_scope
            .difference(&outer_context.vars_possibly_in_scope)
            .copied()
            .collect();

        if has_leaving_statements {
            // A leaving branch (break/continue aside) could still have defined
            // variables seen by code after the enclosing loop.
            if else_context.inside_loop {
                if let Some(loop_scope) = analysis_data.loop_scopes.last_mut() {
                    if !has_continue_statement && !has_break_statement {
                        if_scope
                            .new_vars_possibly_in_scope
                            .extend(vars_possibly_in_scope.iter().copied());
                    }
                    loop_scope
                        .vars_possibly_in_scope
                        .extend(vars_possibly_in_scope);
                }
            }
        } else {
            if_scope
                .new_vars_possibly_in_scope
                .extend(vars_possibly_in_scope);
            if_scope
                .possibly_assigned_var_ids
                .extend(new_possibly_assigned_var_ids);
        }
    }

    // Track references set in the else so they aren't reused later.
    outer_context.update_references_possibly_from_confusing_scope(else_context);

    Ok(())
}
