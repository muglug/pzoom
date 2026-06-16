//! Else-if clause analysis.
//!
//! Faithful port of Psalm's `Block/IfElse/ElseIfAnalyzer::analyze`. Each `elseif`
//! re-enters the conditional analyzer in the running fallthrough (`else_context`),
//! builds the clause formula for its condition, checks it against the entry
//! clauses for paradoxes, narrows the truthy branch, analyses the body, folds the
//! result into the shared [`IfScope`], and finally threads the negated condition
//! into the next fallthrough branch.
//!
//! As in [`else_analyzer`](super::else_analyzer), pzoom's [`BlockContext`] lacks
//! Psalm's `vars_possibly_in_scope`/`loop_scope`/`collect_exceptions`, so those
//! steps are approximated and noted inline. The `assigned_in_conditional`/
//! `entry_clauses` data that Psalm's `IfConditionalScope` carries is reconstructed
//! from the pre-conditional context, since pzoom's scope struct does not expose it.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::statement::Statement;
use rustc_hash::{FxHashMap, FxHashSet};

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{
    Clause, ClauseKey, combine_ored_clauses, get_truths_from_formula, negate_formula, simplify_cnf,
};

use pzoom_code_info::{TAtomic, TUnion};

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::formula_generator;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::scope::IfScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_conditional_analyzer;
use crate::stmt::if_else_analyzer::{
    condition_contains_unsafe_empty_construct, condition_has_assignments,
    infer_condition_truthiness_from_clauses, update_if_scope,
};
use crate::stmt::scope_analyzer::{self, ControlAction};
use crate::stmt_analyzer::analyze_stmts;

/// Mirrors `ElseIfAnalyzer::analyze`. Mutates `if_scope`, `else_context` and
/// `outer_context` in place.
pub(crate) fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    elseif_cond: &Expression<'_>,
    elseif_stmts: &[Statement<'_>],
    if_scope: &mut IfScope,
    else_context: &mut BlockContext,
    outer_context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) -> Result<(), AnalysisError> {
    let pre_conditional_context = else_context.clone();

    // Psalm's IfConditionalAnalyzer ("used when evaluating elseifs") narrows a
    // local clone of the fallthrough context by the accumulated negated truths
    // before analyzing the elseif condition, so e.g. `if ($x === null) {}
    // elseif (foo($x))` sees `$x` non-null inside the condition. The running
    // `else_context` itself stays un-reconciled (its redefinition baseline must
    // keep the pre-narrowing types). Clauses about the changed vars are
    // filtered out of the entry clauses below.
    let mut entry_context = else_context.clone();
    let mut entry_changed_var_ids = FxHashSet::default();
    if !if_scope.negated_clauses.is_empty() && !if_scope.negated_types.is_empty() {
        let negated_types = if_scope.negated_types.clone();
        let inside_loop = entry_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &negated_types,
            &mut entry_context,
            &mut entry_changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::Silent,
            None,
        );
    }

    // Analyze the elseif condition in the (negation-narrowed) fallthrough
    // context. The shared body context becomes the truthy branch we narrow into.
    let if_conditional_scope =
        if_conditional_analyzer::analyze(analyzer, elseif_cond, analysis_data, &entry_context);
    let mut elseif_context = if_conditional_scope.if_body_context;
    let post_cond_context = if_conditional_scope.post_if_context;

    // Reconstruct `assigned_in_conditional_var_ids`: anything whose assignment
    // count grew while analysing the condition.
    let assigned_in_conditional_var_ids: FxHashMap<VarName, usize> = post_cond_context
        .assigned_var_ids
        .iter()
        .filter(|(var_id, count)| {
            pre_conditional_context
                .assigned_var_ids
                .get(*var_id)
                .map_or(true, |pre| *count > pre)
        })
        .map(|(var_id, count)| (var_id.clone(), *count))
        .collect();

    // pzoom's conditional analyzer takes the outer context immutably, so condition
    // side effects (e.g. `elseif ($x = foo())`) are not written back. Thread the
    // condition's assignments into the running fallthrough so the next branch sees
    // them, mirroring Psalm passing `else_context` by reference.
    for var_id in assigned_in_conditional_var_ids.keys() {
        if let Some(ty) = post_cond_context.locals.get(var_id) {
            else_context.locals.insert(var_id.clone(), ty.clone());
        }
        if let Some(count) = post_cond_context.assigned_var_ids.get(var_id) {
            else_context.assigned_var_ids.insert(var_id.clone(), *count);
        }
    }

    let elseif_cond_id = (
        elseif_cond.start_offset() as u32,
        elseif_cond.end_offset() as u32,
    );

    // Variables that are still `mixed` in the truthy branch — clause possibilities
    // narrowing one of their offsets cannot be trusted.
    let mut mixed_var_ids: Vec<String> = elseif_context
        .locals
        .iter()
        .filter(|(_, ty)| ty.is_mixed())
        .map(|(var_id, _)| var_id.to_string())
        .collect();

    let mut elseif_clauses = formula_generator::get_formula(
        elseif_cond_id,
        elseif_cond_id,
        elseif_cond,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();

    if elseif_clauses.len() > 200 {
        elseif_clauses = vec![];
    }

    // Replace clauses that narrow an offset of a still-mixed variable with a wedge.
    let mut elseif_clauses_handled = Vec::with_capacity(elseif_clauses.len());
    for clause in elseif_clauses {
        let keys: Vec<&str> = clause
            .possibilities
            .keys()
            .filter_map(clause_key_name)
            .collect();

        mixed_var_ids.retain(|mixed| !keys.iter().any(|key| key == mixed));

        let references_mixed_offset = keys.iter().any(|key| {
            mixed_var_ids
                .iter()
                .any(|mixed| key_has_root_offset(key, mixed))
        });

        if references_mixed_offset {
            elseif_clauses_handled.push(wedge_clause(elseif_cond_id));
        } else {
            elseif_clauses_handled.push(clause);
        }
    }
    let elseif_clauses = elseif_clauses_handled;

    // entry_clauses: the clauses on entry to the elseif, with any clause that
    // references a variable assigned during the condition invalidated to a wedge.
    // Single-variable clauses about a var the negated-truths reconcile above
    // changed are dropped entirely (Psalm's IfConditionalAnalyzer entry-clause
    // filter) — they have been consumed by that narrowing.
    let mut entry_clauses: Vec<Clause> = Vec::new();
    for clause in pre_conditional_context.clauses.iter() {
        if !entry_changed_var_ids.is_empty()
            && !clause.wedge
            && clause.possibilities.len() == 1
            && clause
                .possibilities
                .keys()
                .next()
                .and_then(clause_key_name)
                .is_some_and(|name| entry_changed_var_ids.contains(name))
        {
            continue;
        }

        let references_assigned = clause.possibilities.keys().any(|key| {
            clause_key_name(key).is_some_and(|name| {
                assigned_in_conditional_var_ids.keys().any(|assigned_id| {
                    let assigned = assigned_id.as_str();
                    name == assigned || key_has_root_offset(name, assigned)
                })
            })
        });

        if references_assigned {
            entry_clauses.push(wedge_clause(elseif_cond_id));
        } else {
            entry_clauses.push((**clause).clone());
        }
    }

    // Detect an elseif condition that is always truthy/falsy given the prior
    // branches (e.g. `if ($a) {} elseif ($a) {}` can never enter the elseif).
    // Mirrors the if branch's diagnostics so the elseif yields the same
    // TypeDoesNotContainType / RedundantCondition as Psalm.
    let elseif_assertion_result =
        assertion_finder::get_assertions(analyzer, elseif_cond, analysis_data);
    let mut condition_is_always = None;
    if !condition_has_assignments(elseif_cond)
        && !condition_contains_unsafe_empty_construct(elseif_cond)
    {
        condition_is_always = infer_condition_truthiness_from_clauses(
            &pre_conditional_context.clauses,
            &elseif_assertion_result,
        );
        if let Some(is_truthy) = condition_is_always {
            analysis_data.expr_types.insert(
                elseif_cond_id,
                Rc::new(if is_truthy {
                    TUnion::new(TAtomic::TTrue)
                } else {
                    TUnion::new(TAtomic::TFalse)
                }),
            );
        }
    }
    if !condition_has_assignments(elseif_cond) {
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            elseif_cond,
            elseif_cond_id,
            analysis_data,
            true,
            Some(&pre_conditional_context),
        );
    }

    // Report paradoxes between the entry clauses and the elseif condition — unless
    // the always-truthy/falsy check above already flagged it, to avoid emitting
    // both a TypeDoesNotContainType and a ParadoxicalCondition for the same spot.
    if condition_is_always.is_none() {
        let entry_clause_refs: Vec<Rc<Clause>> =
            entry_clauses.iter().cloned().map(Rc::new).collect();
        crate::algebra_analyzer::check_for_paradox(
            analyzer,
            &entry_clause_refs,
            &elseif_clauses,
            analysis_data,
            elseif_cond_id,
        );
    }

    let simplified_elseif_clauses = simplify_cnf(elseif_clauses.iter().collect());

    elseif_context.clauses = if !entry_clauses.is_empty() {
        let mut combined = entry_clauses.clone();
        combined.extend(simplified_elseif_clauses.clone());
        simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(Rc::new)
            .collect()
    } else {
        simplified_elseif_clauses
            .iter()
            .cloned()
            .map(Rc::new)
            .collect()
    };

    // Drop any clauses already reconciled while analysing the condition (so an
    // `&&`/ternary-applied assertion isn't re-reported by the body reconcile).
    if !elseif_context.reconciled_expression_clauses.is_empty() {
        let reconciled: FxHashSet<u32> = elseif_context
            .reconciled_expression_clauses
            .iter()
            .map(|c| c.hash)
            .collect();
        elseif_context.clauses = elseif_context
            .clauses
            .iter()
            .filter(|c| !reconciled.contains(&c.hash))
            .cloned()
            .collect();
    }

    let mut cond_referenced_var_ids = FxHashSet::default();
    let (reconcilable_elseif_types, active_elseif_types) = get_truths_from_formula(
        elseif_context.clauses.iter().map(|c| c.as_ref()).collect(),
        Some(elseif_cond_id),
        &mut cond_referenced_var_ids,
    );

    let mut negated_cond_referenced = FxHashSet::default();
    let negated_elseif_types = match negate_formula(elseif_clauses.clone()) {
        Ok(negated) => {
            get_truths_from_formula(negated.iter().collect(), None, &mut negated_cond_referenced).0
        }
        Err(_) => BTreeMap::new(),
    };

    // Merge the negated types into the if_scope so later branches narrow on the
    // assumption that every prior condition was false.
    for (var_id, groups) in &negated_elseif_types {
        match if_scope.negated_types.get_mut(var_id) {
            Some(existing) => existing.extend(groups.iter().cloned()),
            None => {
                if_scope
                    .negated_types
                    .insert(var_id.clone(), groups.clone());
            }
        }
    }

    let mut newly_reconciled_var_ids = FxHashSet::default();
    if !reconcilable_elseif_types.is_empty() {
        let inside_loop = elseif_context.inside_loop;
        // Psalm's reconciler issues point at the elseif condition expression.
        let previous_reconcile_pos = analysis_data.current_reconcile_pos;
        analysis_data.current_reconcile_pos = Some(elseif_cond_id);
        reconciler::reconcile_keyed_types(
            &reconcilable_elseif_types,
            &mut elseif_context,
            &mut newly_reconciled_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::All,
            Some(&active_elseif_types),
        );
        analysis_data.current_reconcile_pos = previous_reconcile_pos;

        if !newly_reconciled_var_ids.is_empty() {
            elseif_context.clauses = BlockContext::remove_reconciled_clause_refs(
                &elseif_context.clauses,
                &newly_reconciled_var_ids,
            )
            .0;
        }
    }

    let pre_stmts_assigned_var_ids = std::mem::take(&mut elseif_context.assigned_var_ids);
    let pre_stmts_possibly_assigned_var_ids =
        std::mem::take(&mut elseif_context.possibly_assigned_var_ids);

    // Baseline for body-removed vars (see update_if_scope).
    let post_condition_locals = elseif_context.locals.clone();

    analyze_stmts(analyzer, elseif_stmts, analysis_data, &mut elseif_context)?;

    // Propagate branch-local clause evictions to the outer context (Psalm's
    // `parent_remove_vars` loop).
    for var_name in elseif_context.parent_remove_vars.clone() {
        outer_context.remove_var_name_from_conflicting_clauses(&var_name);
    }

    let new_stmts_assigned_var_ids = elseif_context.assigned_var_ids.clone();
    for (var_id, count) in pre_stmts_assigned_var_ids {
        elseif_context
            .assigned_var_ids
            .entry(var_id)
            .or_insert(count);
    }
    let new_stmts_possibly_assigned_var_ids = elseif_context.possibly_assigned_var_ids.clone();
    elseif_context
        .possibly_assigned_var_ids
        .extend(pre_stmts_possibly_assigned_var_ids);

    // Carry by-ref constraints discovered in the elseif into the outer context,
    // reporting conflicts (Psalm's byref_constraints merge).
    {
        let elseif_span = mago_span::HasSpan::span(elseif_cond);
        let elseif_pos = (elseif_span.start.offset, elseif_span.end.offset);
        let branch_constraints = elseif_context.clone();
        crate::stmt::if_else_analyzer::carry_reference_constraints_to_outer(
            analyzer,
            analysis_data,
            outer_context,
            &branch_constraints,
            elseif_pos,
        );
    }

    let final_actions = scope_analyzer::get_control_actions(elseif_stmts, analysis_data, &[], true);
    let has_ending_statements =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::End);
    let has_leaving_statements = has_ending_statements
        || (!final_actions.is_empty() && !final_actions.contains(&ControlAction::None));
    let has_break_statement =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Break);
    let has_continue_statement =
        final_actions.len() == 1 && final_actions.contains(&ControlAction::Continue);

    if_scope.final_actions.extend(final_actions.iter().copied());

    if !has_leaving_statements {
        let mut merged_assigned = new_stmts_assigned_var_ids.clone();
        for (var_id, count) in &assigned_in_conditional_var_ids {
            merged_assigned.entry(var_id.clone()).or_insert(*count);
        }

        update_if_scope(
            analyzer,
            if_scope,
            &elseif_context,
            outer_context,
            &post_condition_locals,
            &merged_assigned,
            &new_stmts_possibly_assigned_var_ids,
            &newly_reconciled_var_ids,
            true,
        );

        let reasonable_clause_count = if_scope.reasonable_clauses.len();
        if reasonable_clause_count > 0
            && reasonable_clause_count < 20_000
            && !elseif_clauses.is_empty()
        {
            let existing: Vec<Clause> = if_scope
                .reasonable_clauses
                .iter()
                .map(|c| (**c).clone())
                .collect();
            if_scope.reasonable_clauses =
                combine_ored_clauses(existing, elseif_clauses.clone(), elseif_cond_id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(Rc::new)
                    .collect();
        } else {
            if_scope.reasonable_clauses = vec![];
        }
    } else {
        if_scope.reasonable_clauses = vec![];
    }

    // When the elseif leaves, the negated condition implies new facts for the
    // fallthrough: reconcile them against the pre-conditional context so any
    // resulting contradictions are reported.
    if !negated_elseif_types.is_empty() && has_leaving_statements {
        let mut implied_outer_context = pre_conditional_context.clone();
        let inside_loop = elseif_context.inside_loop;
        let mut implied_changed = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &negated_elseif_types,
            &mut implied_outer_context,
            &mut implied_changed,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::Silent,
            None,
        );
    }

    if !has_ending_statements {
        let vars_possibly_in_scope: FxHashSet<_> = elseif_context
            .vars_possibly_in_scope
            .difference(&outer_context.vars_possibly_in_scope)
            .cloned()
            .collect();

        if has_leaving_statements && elseif_context.inside_loop {
            if let Some(loop_scope) = analysis_data.loop_scopes.last_mut() {
                if !has_continue_statement && !has_break_statement {
                    if_scope
                        .new_vars_possibly_in_scope
                        .extend(vars_possibly_in_scope.iter().cloned());
                    if_scope
                        .possibly_assigned_var_ids
                        .extend(new_stmts_possibly_assigned_var_ids.iter().cloned());
                }
                loop_scope
                    .vars_possibly_in_scope
                    .extend(vars_possibly_in_scope);
            }
        } else if !has_leaving_statements {
            if_scope
                .new_vars_possibly_in_scope
                .extend(vars_possibly_in_scope);
            if_scope
                .possibly_assigned_var_ids
                .extend(new_stmts_possibly_assigned_var_ids);
        }
    }

    // Accumulate the negated elseif condition for the next fallthrough branch.
    match negate_formula(elseif_clauses.clone()) {
        Ok(negated) => {
            let mut combined = if_scope.negated_clauses.clone();
            combined.extend(negated.clone());
            if_scope.negated_clauses = simplify_cnf(combined.iter().collect());

            // Also thread the negation directly into the running fallthrough
            // context so the next branch narrows on it (pzoom's BlockContext does
            // not re-derive this from if_scope the way Psalm's does).
            let mut else_clauses: Vec<Clause> =
                else_context.clauses.iter().map(|c| (**c).clone()).collect();
            else_clauses.extend(negated);
            else_context.clauses = simplify_cnf(else_clauses.iter().collect())
                .into_iter()
                .map(Rc::new)
                .collect();
        }
        Err(_) => {
            if_scope.negated_clauses = vec![];
        }
    }

    // Track references set in the elseif so they aren't reused later.
    outer_context.update_references_possibly_from_confusing_scope(&elseif_context);

    Ok(())
}

/// The name of a named clause key, if it is one.
fn clause_key_name(key: &ClauseKey) -> Option<&str> {
    match key {
        ClauseKey::Name(name) => Some(name.as_str()),
        ClauseKey::Range(_, _) => None,
    }
}

/// Whether `key` is an array/property offset rooted at `root` (e.g. `$a[0]` or
/// `$a->b` for root `$a`). Mirrors Psalm's `/^root(\[|-)/` check.
fn key_has_root_offset(key: &str, root: &str) -> bool {
    key.len() > root.len()
        && key.starts_with(root)
        && matches!(key.as_bytes()[root.len()], b'[' | b'-')
}

/// An empty, reconcilable wedge clause — Psalm's `new Clause([], $id, $id, true)`.
fn wedge_clause(cond_id: (u32, u32)) -> Clause {
    Clause::new(BTreeMap::new(), cond_id, cond_id, Some(true), None, None)
}
