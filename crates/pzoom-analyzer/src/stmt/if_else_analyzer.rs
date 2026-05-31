//! If/elseif/else statement analyzer (Psalm `IfElseAnalyzer` equivalent).
//!
//! This is the statement-level orchestrator: it analyzes the `if` condition and
//! branch, then each elseif and the else, tracking CNF formulas through nested
//! conditions for type narrowing and merging the branch contexts. Mirrors
//! Psalm's `IfElseAnalyzer` (which delegates the per-branch bodies to
//! `IfAnalyzer`/`ElseIfAnalyzer`/`ElseAnalyzer`; pzoom keeps the `if` branch
//! inline and uses `elseif_analyzer`/`else_analyzer` for the rest).

use mago_span::HasSpan;
use mago_syntax::ast::ast::access::Access;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::call::Call;
use mago_syntax::ast::ast::construct::Construct;
use mago_syntax::ast::ast::control_flow::r#if::If;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::unary::UnaryPrefixOperator;
use mago_syntax::ast::ast::variable::Variable;
use std::collections::BTreeMap;

use pzoom_code_info::algebra::{
    Clause, get_truths_from_formula, negate_formula, simplify_cnf,
};
use pzoom_code_info::{Assertion, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::scope::IfScope;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::scope_analyzer::{self, ControlAction};
use crate::stmt::if_conditional_analyzer;
use crate::stmt::{else_analyzer, elseif_analyzer};
use crate::stmt_analyzer::analyze_stmts;

/// Analyze an if statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    if_stmt: &If<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition through the conditional scope. The &&/|| analyzers narrow
    // into a shared if_body_context (including right-operand assignments); the
    // returned post_if_context is the fallthrough base used for the else branch.
    let cond_pos = (
        if_stmt.condition.start_offset() as u32,
        if_stmt.condition.end_offset() as u32,
    );
    let if_conditional_scope =
        if_conditional_analyzer::analyze(analyzer, if_stmt.condition, analysis_data, context);
    let cond_if_body_context = if_conditional_scope.if_body_context;
    *context = if_conditional_scope.post_if_context;

    // Snapshot the post-condition locals before the if branch's `Context::update`
    // can narrow/insert into them — Psalm clones `else_context` from
    // `post_if_context` up front, so the else branch sees the fallthrough state,
    // not values leaked from the if body (e.g. `$options['b'] = 1`).
    let post_if_locals = context.locals.clone();

    // Get type narrowing assertions from the condition using the assertion finder
    let assertion_result =
        assertion_finder::get_assertions(analyzer, if_stmt.condition, analysis_data);
    if !condition_has_assignments(if_stmt.condition)
        && !condition_contains_unsafe_empty_construct(if_stmt.condition)
    {
        if let Some(condition_is_always_truthy) =
            infer_condition_truthiness_from_clauses(&context.clauses, &assertion_result)
        {
            analysis_data.set_expr_type(
                cond_pos,
                if condition_is_always_truthy {
                    TUnion::new(TAtomic::TTrue)
                } else {
                    TUnion::new(TAtomic::TFalse)
                },
            );
        }
    }
    if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        if_stmt.condition,
        cond_pos,
        analysis_data,
        true,
        Some(context),
    );
    if !condition_has_assignments(if_stmt.condition) {
        emit_active_assertion_contradictions(
            analyzer,
            context,
            &assertion_result.if_true,
            &cond_if_body_context.reconciled_expression_clauses,
            analysis_data,
        );
    }

    let if_conditional_id = (
        if_stmt.condition.start_offset() as u32,
        if_stmt.condition.end_offset() as u32,
    );

    // Check whether the condition is redundant with, or contradicts, the clauses
    // already established in this context (Hakana's algebra_analyzer::check_for_paradox).
    if !condition_has_assignments(if_stmt.condition) {
        let if_formula = crate::formula_generator::get_formula(
            if_conditional_id,
            if_conditional_id,
            if_stmt.condition,
            analyzer,
            analysis_data,
            false,
        )
        .unwrap_or_default();
        crate::algebra_analyzer::check_for_paradox(
            analyzer,
            &context.clauses,
            &if_formula,
            analysis_data,
            cond_pos,
        );
    }

    // Analyze the if branch with type narrowing
    let mut if_context = context.child();
    seed_assignment_tracking(context, &mut if_context);
    // Seed the if body from the operator-narrowed conditional scope (matching
    // hakana's if_body_context), before the clause-based reconciliation narrows more.
    if_context.locals = cond_if_body_context.locals.clone();
    for (var_id, count) in &cond_if_body_context.assigned_var_ids {
        if_context.assigned_var_ids.entry(*var_id).or_insert(*count);
    }

    // Combine parent clauses with the new if-true clauses
    let mut if_clauses: Vec<_> = context.clauses.iter().map(|c| (**c).clone()).collect();
    if_clauses.extend(assertion_result.if_true_clauses.clone());
    if_context.clauses = if_clauses.into_iter().map(std::rc::Rc::new).collect();

    // Apply assertions to if branch context using the reconciler
    apply_clauses_to_context(
        analyzer,
        &mut if_context,
        analysis_data,
        Some(if_conditional_id),
        false,
    );
    promote_asserted_vars_to_assigned(analyzer, &assertion_result.if_true, &mut if_context);
    apply_correlated_equality_narrowing(analyzer, if_stmt.condition, &mut if_context, true);
    promote_guaranteed_true_condition_assignments(analyzer, if_stmt.condition, &mut if_context);

    // Variables the condition narrowed entering the if body (Psalm's
    // `if_cond_changed_var_ids`), captured before the body mutates anything.
    let if_cond_changed_var_ids = BlockContext::get_new_or_updated_locals(context, &if_context);

    // The if context as it stands after condition narrowing but before the body —
    // Psalm's `$old_if_context`, the baseline for `Context::update`.
    let old_if_context = if_context.clone();

    // Snapshot assignment tracking so we can isolate what the if body assigns.
    let pre_if_assigned_var_ids = if_context.assigned_var_ids.clone();
    let pre_if_possibly_assigned_var_ids = if_context.possibly_assigned_var_ids.clone();

    // Analyze the if body
    let if_stmts = if_stmt.body.statements();
    analyze_stmts(analyzer, if_stmts, analysis_data, &mut if_context)?;


    // Determine the if branch's control actions.
    let if_actions = scope_analyzer::get_control_actions(if_stmts, analysis_data, &[], true);
    let if_exits = !if_actions.contains(&ControlAction::None);
    let has_ending_if = if_actions.len() == 1 && if_actions.contains(&ControlAction::End);

    // The if condition's CNF formula (formula-generator based, so disjunctions like
    // `$a || $b` are represented faithfully — unlike the assertion-finder clauses
    // pzoom uses to narrow the if body).
    let if_clauses = crate::formula_generator::get_formula(
        if_conditional_id,
        if_conditional_id,
        if_stmt.condition,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();


    // reasonable_clauses = the clauses true on the if path (parent ∧ condition).
    // Built from the formula so a continuing-if/leaving-else like
    // `if ($a || $b) {} else { return; }` carries `$a || $b` to the outer context.
    let reasonable_clauses: Vec<std::rc::Rc<Clause>> = {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_clauses.iter().cloned());
        simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect()
    };


    // ----- Build the IfScope (Psalm `IfElseAnalyzer` setup) -----
    let mut if_scope = IfScope {
        if_actions: if_actions.clone(),
        final_actions: if_actions.clone(),
        if_cond_changed_var_ids,
        reasonable_clauses,
        ..IfScope::default()
    };

    // negated_clauses = negateFormula(if_clauses); fall back to an empty formula.
    if_scope.negated_clauses = negate_formula(if_clauses.clone()).unwrap_or_default();

    // negated_types = truths of simplifyCNF(context.clauses + negated_clauses).
    {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        let simplified = simplify_cnf(combined.iter().collect());
        let mut referenced = FxHashSet::default();
        if_scope.negated_types =
            get_truths_from_formula(simplified.iter().collect(), None, &mut referenced).0;
    }

    // negatable_if_types: only the variables whose negation is a *definite* per-var
    // fact (single-possibility clause) are safe to push back through
    // `BlockContext::update`. A disjunctive negation such as `$a || $b` (from
    // `if (!$a && !$b)`) yields no truths here, so neither var is negated per-var —
    // that fact is carried as a clause instead. Restrict to the condition's own
    // variables so unrelated context truths aren't substituted.
    {
        let mut negatable = FxHashSet::default();
        for var_name in if_scope.negated_types.keys() {
            if let Some(var_id) = analyzer.interner.find(var_name) {
                if if_scope.if_cond_changed_var_ids.contains(&var_id) {
                    negatable.insert(var_id);
                }
            }
        }
        if_scope.negatable_if_types = negatable;
    }

    // Fold the if branch into the if_scope when it can fall through
    // (Psalm `IfAnalyzer::updateIfScope`). Isolate what the if body assigned.
    let new_if_assigned_var_ids: FxHashMap<StrId, usize> = if_context
        .assigned_var_ids
        .iter()
        .filter(|(var_id, count)| {
            pre_if_assigned_var_ids
                .get(*var_id)
                .map_or(true, |pre| *count > pre)
        })
        .map(|(var_id, count)| (*var_id, *count))
        .collect();
    let new_if_possibly_assigned_var_ids: FxHashSet<StrId> = if_context
        .possibly_assigned_var_ids
        .difference(&pre_if_possibly_assigned_var_ids)
        .copied()
        .collect();

    if !if_exits {
        let if_cond_changed = if_scope.if_cond_changed_var_ids.clone();
        update_if_scope(
            analyzer,
            &mut if_scope,
            &if_context,
            context,
            &new_if_assigned_var_ids,
            &new_if_possibly_assigned_var_ids,
            &if_cond_changed,
            true,
        );

        if !has_ending_if {
            let vars_possibly_in_scope: FxHashSet<_> = if_context
                .vars_possibly_in_scope
                .difference(&context.vars_possibly_in_scope)
                .copied()
                .collect();
            if_scope
                .new_vars_possibly_in_scope
                .extend(vars_possibly_in_scope);
            if_scope
                .possibly_assigned_var_ids
                .extend(new_if_possibly_assigned_var_ids.iter().copied());
        }
    }

    // Propagate the if-condition's narrowing into the outer context. When the if
    // branch leaves, this strips the now-impossible (condition-true) possibility
    // from the outer types (`if (!$a) return;` ⇒ `$a` truthy afterwards); Psalm's
    // `$outer_context->update(...)` gated on negated_types.
    if !if_scope.negated_types.is_empty() && !if_scope.negatable_if_types.is_empty() {
        let negatable = if_scope.negatable_if_types.clone();
        let mut updated_vars = std::mem::take(&mut if_scope.updated_vars);
        context.update(
            &old_if_context,
            &if_context,
            if_exits,
            &negatable,
            &mut updated_vars,
        );
        if_scope.updated_vars = updated_vars;
    }

    context.update_references_possibly_from_confusing_scope(&if_context);

    // ----- Build the fallthrough context and drive elseif/else branches -----
    let mut else_context = context.child();
    // Reset to the pre-if-body fallthrough locals so the else/elseif branches don't
    // inherit values the if body (via `update`) wrote into the outer context.
    else_context.locals = post_if_locals.clone();
    seed_assignment_tracking(context, &mut else_context);
    {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        else_context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
    }
    if !if_scope.negated_types.is_empty() {
        let inside_loop = else_context.inside_loop;
        let negated_types = if_scope.negated_types.clone();
        let mut changed = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &negated_types,
            &mut else_context,
            &mut changed,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            false,
            None,
        );
    }

    // The assertion finder narrows some negations the formula→reconcile path
    // doesn't (e.g. `get_class($a) !== B` ⇒ instanceof B). With no elseifs the if's
    // `if_false` assertions are exactly the else path's facts, so apply them too.
    let has_elseif_clauses = !if_stmt.body.else_if_clauses().is_empty();
    if !has_elseif_clauses && !assertion_result.if_false.is_empty() {
        let inside_loop = else_context.inside_loop;
        let mut changed = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &reconciler::to_and_groups(&assertion_result.if_false),
            &mut else_context,
            &mut changed,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            false,
            None,
        );
    }

    let mut all_elseifs_leave = true;
    let mut has_elseifs = false;
    for (elseif_cond, elseif_stmts) in if_stmt.body.else_if_clauses() {
        has_elseifs = true;
        let elseif_actions = scope_analyzer::get_control_actions(elseif_stmts, analysis_data, &[], true);
        all_elseifs_leave = all_elseifs_leave && !elseif_actions.contains(&ControlAction::None);
        elseif_analyzer::analyze(
            analyzer,
            elseif_cond,
            elseif_stmts,
            &mut if_scope,
            &mut else_context,
            context,
            analysis_data,
        )?;
    }

    // The else's per-variable `update()` is only sound for a simple if/else: with
    // intervening elseif branches the fallthrough can still reach the continuation
    // with the variable at its else-path value, so narrowing it away (when the else
    // leaves) would be unsound. The branch merge handles those cases instead.
    if has_elseifs {
        if_scope.negatable_if_types.clear();
    }

    else_analyzer::analyze(
        analyzer,
        if_stmt.body.else_statements(),
        &mut if_scope,
        &mut else_context,
        context,
        analysis_data,
    )?;

    // When there is no explicit `else` and every condition branch leaves (returns,
    // throws, …), reaching the code after the construct implies every branch
    // condition was false. Carry the accumulated negated clauses into the outer
    // context so a disjunctive fact like `$a || $b` (from `if (!$a && !$b) return;`)
    // survives to narrow later statements. (Psalm threads this via the
    // post-leaving-if continuation context.)
    if !if_stmt.body.has_else_clause() && if_exits && all_elseifs_leave {
        let mut combined: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
        combined.extend(if_scope.negated_clauses.iter().cloned());
        context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
        apply_clauses_to_context(analyzer, context, analysis_data, None, false);

        // The assertion finder narrows some negations the formula→reconcile path
        // doesn't (e.g. `!== ""` ⇒ non-empty-string, `get_class($a) !== B` ⇒
        // instanceof B). When there are no elseifs the if's `if_false` assertions
        // are exactly the continuation's facts, so apply them directly too.
        if !has_elseifs && !assertion_result.if_false.is_empty() {
            let inside_loop = context.inside_loop;
            let mut changed = FxHashSet::default();
            reconciler::reconcile_keyed_types(
                &reconciler::to_and_groups(&assertion_result.if_false),
                context,
                &mut changed,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                false,
                None,
            );
            promote_asserted_vars_to_assigned(analyzer, &assertion_result.if_false, context);
        }
    }


    // ----- Apply the IfScope back into the outer context (Psalm tail) -----
    context
        .possibly_assigned_var_ids
        .extend(if_scope.new_vars_possibly_in_scope.iter().copied());
    context
        .possibly_assigned_var_ids
        .extend(if_scope.possibly_assigned_var_ids.iter().copied());
    if let Some(assigned) = &if_scope.assigned_var_ids {
        for (var_id, count) in assigned {
            context.assigned_var_ids.insert(*var_id, *count);
        }
    }

    if let Some(new_vars) = if_scope.new_vars.clone() {
        for (var_id, var_type) in new_vars {
            context.locals.insert(var_id, var_type);
        }
    }

    if let Some(redefined_vars) = if_scope.redefined_vars.clone() {
        for (var_id, var_type) in redefined_vars {
            context.locals.insert(var_id, var_type);
            if_scope.updated_vars.insert(var_id);

            // Psalm's Context::filterClauses: a redefined var's prior clauses can
            // no longer be trusted, so drop the ones that mention it. (Variables
            // only narrowed by a condition fall back to their outer type during the
            // branch merge and so are not present here.)
            if !if_scope.reasonable_clauses.is_empty() {
                let mut changed = FxHashSet::default();
                changed.insert(var_id);
                if_scope.reasonable_clauses = BlockContext::remove_reconciled_clause_refs(
                    &if_scope.reasonable_clauses,
                    &changed,
                    analyzer.interner,
                )
                .0;
            }
        }
    }


    if !if_scope.reasonable_clauses.is_empty()
        && (if_scope.reasonable_clauses.len() > 1 || !if_scope.reasonable_clauses[0].wedge)
    {
        let mut combined: Vec<Clause> = if_scope
            .reasonable_clauses
            .iter()
            .map(|c| (**c).clone())
            .collect();
        combined.extend(context.clauses.iter().map(|c| (**c).clone()));
        context.clauses = simplify_cnf(combined.iter().collect())
            .into_iter()
            .map(std::rc::Rc::new)
            .collect();
    }

    for (var_id, var_type) in if_scope.possibly_redefined_vars.clone() {
        if context.locals.contains_key(&var_id) && !if_scope.updated_vars.contains(&var_id) {
            let existing = context.locals.get(&var_id).cloned().unwrap();
            let combined = combine_union_types(&existing, &var_type, false);
            if combined != existing {
                context.invalidate_dependent_types(var_id);
            }
            context.locals.insert(var_id, combined);
        }
    }






    // If no branch can fall through, control never reaches the code after the if.
    if !if_scope.final_actions.contains(&ControlAction::None) {
        context.has_returned = true;
    }


    Ok(())
}

/// Port of Psalm's `IfAnalyzer::updateIfScope`: folds one branch's redefinitions
/// into the shared [`IfScope`]. New variables are intersected across branches and
/// their types unioned; redefined variables are unioned; "possibly redefined"
/// tracks variables changed in only some branches.
#[allow(clippy::too_many_arguments)]
pub(crate) fn update_if_scope(
    _analyzer: &StatementsAnalyzer<'_>,
    if_scope: &mut IfScope,
    branch_context: &BlockContext,
    outer_context: &BlockContext,
    assigned_var_ids: &FxHashMap<StrId, usize>,
    possibly_assigned_var_ids: &FxHashSet<StrId>,
    newly_reconciled_var_ids: &FxHashSet<StrId>,
    update_new_vars: bool,
) {
    let mut removed_vars = FxHashSet::default();
    let mut redefined_vars =
        branch_context.get_redefined_locals(&outer_context.locals, false, &mut removed_vars);

    // The `||`/`&&` analyzers can re-emit a variable's type with fresh data-flow
    // nodes but the same structure; treating that as a redefinition would wrongly
    // invalidate clauses about the variable (e.g. drop `$a || $b` after an empty
    // `if ($a || $b)` body). Keep only genuine structural changes.
    redefined_vars.retain(|var_id, branch_type| {
        outer_context
            .locals
            .get(var_id)
            .map_or(true, |outer_type| {
                !crate::context::unions_structurally_equal(branch_type, outer_type)
            })
    });

    match &mut if_scope.new_vars {
        None => {
            if update_new_vars {
                let new_vars: BTreeMap<StrId, TUnion> = branch_context
                    .locals
                    .iter()
                    .filter(|(var_id, _)| !outer_context.locals.contains_key(*var_id))
                    .map(|(var_id, ty)| (*var_id, ty.clone()))
                    .collect();
                if_scope.new_vars = Some(new_vars);
            }
        }
        Some(new_vars) => {
            new_vars.retain(|var_id, _| branch_context.has_variable(*var_id));
            for (var_id, ty) in new_vars.iter_mut() {
                if let Some(branch_ty) = branch_context.locals.get(var_id) {
                    *ty = combine_union_types(ty, branch_ty, false);
                }
            }
        }
    }

    // possibly_redefined = redefined, minus vars only narrowed but never assigned.
    let mut possibly_redefined_vars = redefined_vars.clone();
    possibly_redefined_vars.retain(|var_id, _| {
        !(!possibly_assigned_var_ids.contains(var_id) && newly_reconciled_var_ids.contains(var_id))
    });

    match &mut if_scope.assigned_var_ids {
        None => if_scope.assigned_var_ids = Some(assigned_var_ids.clone()),
        Some(existing) => {
            let intersected: FxHashMap<StrId, usize> = assigned_var_ids
                .iter()
                .filter(|(var_id, _)| existing.contains_key(*var_id))
                .map(|(var_id, count)| (*var_id, *count))
                .collect();
            *existing = intersected;
        }
    }

    for var_id in possibly_assigned_var_ids {
        if_scope.possibly_assigned_var_ids.insert(*var_id);
    }

    match &mut if_scope.redefined_vars {
        None => {
            if_scope.redefined_vars = Some(redefined_vars);
            if_scope.possibly_redefined_vars = possibly_redefined_vars;
        }
        Some(existing) => {
            existing.retain(|var_id, _| redefined_vars.contains_key(var_id));
            for (var_id, ty) in existing.iter_mut() {
                *ty = combine_union_types(&redefined_vars[var_id], ty, false);
            }
            // Once the per-branch types are combined, a variable whose merged type
            // matches the outer type again was not really redefined (e.g. narrowed
            // one way in the `if`, the other in the `elseif`, recombining to the
            // original) — drop it so its clauses survive.
            existing.retain(|var_id, ty| match outer_context.locals.get(var_id) {
                Some(outer_ty) => !crate::context::unions_structurally_equal(ty, outer_ty),
                None => true,
            });
            for (var_id, ty) in possibly_redefined_vars {
                use std::collections::hash_map::Entry;
                match if_scope.possibly_redefined_vars.entry(var_id) {
                    Entry::Occupied(mut occupied) => {
                        let combined = combine_union_types(&ty, occupied.get(), false);
                        occupied.insert(combined);
                    }
                    Entry::Vacant(vacant) => {
                        vacant.insert(ty);
                    }
                }
            }
        }
    }
}

pub(crate) fn promote_asserted_vars_to_assigned(
    analyzer: &StatementsAnalyzer<'_>,
    assertions: &BTreeMap<String, Vec<Assertion>>,
    context: &mut BlockContext,
) {
    for var_name in assertions.keys() {
        if var_name.contains('[')
            || var_name.contains("->")
            || var_name.contains("::")
            || var_name.contains('(')
        {
            continue;
        }

        let mut candidates = vec![var_name.as_str()];
        if let Some(stripped) = var_name.strip_prefix('$') {
            candidates.push(stripped);
        } else {
            let with_dollar = format!("${var_name}");
            if let Some(var_id) = analyzer.interner.find(&with_dollar) {
                if context.locals.contains_key(&var_id) {
                    *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
                    context.possibly_assigned_var_ids.remove(&var_id);
                }
            }
        }

        for candidate in candidates {
            if let Some(var_id) = analyzer.interner.find(candidate) {
                if context.locals.contains_key(&var_id) {
                    *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
                    context.possibly_assigned_var_ids.remove(&var_id);
                }
            }
        }
    }
}

#[derive(Default, Clone)]
struct GuaranteedAssignmentSets {
    when_true: FxHashSet<StrId>,
    when_false: FxHashSet<StrId>,
}

pub(crate) fn promote_guaranteed_true_condition_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    condition: &Expression<'_>,
    context: &mut BlockContext,
) {
    let guaranteed = collect_guaranteed_assignments(analyzer, condition);
    for var_id in guaranteed.when_true {
        if context.locals.contains_key(&var_id) {
            *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
            context.possibly_assigned_var_ids.remove(&var_id);
        }
    }
}

pub(crate) fn seed_assignment_tracking(from: &BlockContext, to: &mut BlockContext) {
    to.assigned_var_ids = from.assigned_var_ids.clone();
    to.possibly_assigned_var_ids = from.possibly_assigned_var_ids.clone();
}

pub(crate) fn condition_has_assignments(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Assignment(_) | Expression::UnaryPostfix(_) => true,
        Expression::Binary(binary) => {
            condition_has_assignments(binary.lhs) || condition_has_assignments(binary.rhs)
        }
        Expression::UnaryPrefix(unary) => condition_has_assignments(unary.operand),
        Expression::Parenthesized(parenthesized) => condition_has_assignments(parenthesized.expression),
        _ => false,
    }
}

pub(crate) fn condition_contains_unsafe_empty_construct(expr: &Expression<'_>) -> bool {
    match expr.unparenthesized() {
        Expression::Construct(Construct::Empty(empty_construct)) => {
            matches!(
                empty_construct.value.unparenthesized(),
                Expression::ArrayAccess(_)
                    | Expression::Access(Access::Property(_))
                    | Expression::Access(Access::NullSafeProperty(_))
            )
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            condition_contains_unsafe_empty_construct(unary.operand)
        }
        Expression::Parenthesized(paren) => {
            condition_contains_unsafe_empty_construct(paren.expression)
        }
        Expression::Binary(binary) => {
            condition_contains_unsafe_empty_construct(binary.lhs)
                || condition_contains_unsafe_empty_construct(binary.rhs)
        }
        _ => false,
    }
}

fn collect_guaranteed_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
) -> GuaranteedAssignmentSets {
    match expr.unparenthesized() {
        Expression::Parenthesized(parenthesized) => {
            collect_guaranteed_assignments(analyzer, parenthesized.expression)
        }
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            let inner = collect_guaranteed_assignments(analyzer, unary.operand);
            GuaranteedAssignmentSets {
                when_true: inner.when_false,
                when_false: inner.when_true,
            }
        }
        Expression::Assignment(assignment) => {
            let mut rhs = collect_guaranteed_assignments(analyzer, assignment.rhs);
            if let Expression::Variable(Variable::Direct(direct)) = assignment.lhs.unparenthesized()
            {
                let var_id = analyzer.interner.intern(direct.name);
                rhs.when_true.insert(var_id);
                rhs.when_false.insert(var_id);
            }
            rhs
        }
        Expression::Call(call) => {
            let by_ref_assignments = collect_call_by_ref_assignments(analyzer, call);
            GuaranteedAssignmentSets {
                when_true: by_ref_assignments.clone(),
                when_false: by_ref_assignments,
            }
        }
        Expression::Binary(binary) => {
            let left = collect_guaranteed_assignments(analyzer, binary.lhs);
            let right = collect_guaranteed_assignments(analyzer, binary.rhs);

            match &binary.operator {
                BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => {
                    // true path: lhs true OR (lhs false and rhs true)
                    let path_true_rhs = union_set(&left.when_false, &right.when_true);
                    // false path: lhs false AND rhs false
                    let false_set = union_set(&left.when_false, &right.when_false);

                    GuaranteedAssignmentSets {
                        when_true: intersect_set(&left.when_true, &path_true_rhs),
                        when_false: false_set,
                    }
                }
                BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => {
                    // true path: lhs true AND rhs true
                    let true_set = union_set(&left.when_true, &right.when_true);
                    // false path: lhs false OR (lhs true and rhs false)
                    let path_false_rhs = union_set(&left.when_true, &right.when_false);

                    GuaranteedAssignmentSets {
                        when_true: true_set,
                        when_false: intersect_set(&left.when_false, &path_false_rhs),
                    }
                }
                _ => {
                    // Non short-circuit expression: both operands are evaluated.
                    let eval_set = union_set(
                        &guaranteed_when_evaluated(&left),
                        &guaranteed_when_evaluated(&right),
                    );
                    GuaranteedAssignmentSets {
                        when_true: eval_set.clone(),
                        when_false: eval_set,
                    }
                }
            }
        }
        _ => GuaranteedAssignmentSets::default(),
    }
}

fn guaranteed_when_evaluated(sets: &GuaranteedAssignmentSets) -> FxHashSet<StrId> {
    intersect_set(&sets.when_true, &sets.when_false)
}

fn collect_call_by_ref_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
) -> FxHashSet<StrId> {
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
            assigned.insert(analyzer.interner.intern(direct.name));
        }
    }

    assigned
}

fn is_preg_match_out_param(function_id: StrId, param_idx: usize) -> bool {
    if param_idx != 2 {
        return false;
    }

    matches!(function_id, StrId::PREG_MATCH | StrId::PREG_MATCH_ALL)
}

fn union_set(left: &FxHashSet<StrId>, right: &FxHashSet<StrId>) -> FxHashSet<StrId> {
    left.union(right).copied().collect()
}

fn intersect_set(left: &FxHashSet<StrId>, right: &FxHashSet<StrId>) -> FxHashSet<StrId> {
    left.intersection(right).copied().collect()
}

/// Apply clauses to a context by simplifying the CNF formula and extracting truths.
pub(crate) fn apply_clauses_to_context(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
    creating_conditional_id: Option<(u32, u32)>,
    emit_redundant_issues: bool,
) {
    if context.clauses.is_empty() {
        return;
    }

    // Simplify the CNF formula
    let clause_refs: Vec<&Clause> = context.clauses.iter().map(|c| c.as_ref()).collect();
    let simplified = simplify_cnf(clause_refs);

    // Extract truths from the simplified formula
    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, active_truths) = get_truths_from_formula(
        simplified.iter().collect(),
        creating_conditional_id,
        &mut cond_referenced_var_ids,
    );

    // `truths` carries OR groups (each inner list is OR-ed, multiple lists are
    // applied conjunctively). reconcile_keyed_types handles both natively, and
    // active_truths is already clause-indexed.
    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        &truths,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        emit_redundant_issues,
        if emit_redundant_issues {
            Some(&active_truths)
        } else {
            None
        },
    );

    // Mirror Psalm: every `Reconciler::reconcileKeyedTypes` is followed by
    // `Context::removeReconciledClauses($clauses, $changed_var_ids)`. Once a
    // clause has been reconciled into a variable's type, retaining it is unsound
    // (the type now carries the fact) and lets it bleed into later constructs —
    // e.g. a disjunctive `$s === "a" || $s === "b"` surviving into a following
    // `switch ($s)`, where simplifying it against a case's negation re-derives the
    // case and wrongly reports `RedundantCondition`.
    if !changed_var_ids.is_empty() {
        context.clauses = BlockContext::remove_reconciled_clause_refs(
            &context.clauses,
            &changed_var_ids,
            analyzer.interner,
        )
        .0;
    }
}

pub(crate) fn emit_active_assertion_contradictions(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    assertions: &BTreeMap<String, Vec<Assertion>>,
    reconciled_expression_clauses: &[std::rc::Rc<Clause>],
    analysis_data: &mut FunctionAnalysisData,
) {
    if assertions.is_empty() {
        return;
    }

    // Mirror Hakana's reconciled_expression_clauses: an assertion an `&&`/`||`/ternary
    // already reconciled during condition analysis still applies (for narrowing
    // sequence) but is no longer *reported* — only the not-yet-reconciled assertions
    // are active. This lets `is_int($c) && is_numeric($c)` apply is_int (silently) and
    // still flag is_numeric as redundant, without re-reporting is_int itself.
    let reconciled_assertions: FxHashSet<(&str, u64)> = reconciled_expression_clauses
        .iter()
        .flat_map(|clause| {
            clause.possibilities.iter().filter_map(|(key, assertions)| {
                if let pzoom_code_info::algebra::ClauseKey::Name(name) = key {
                    Some(assertions.keys().map(move |hash| (name.as_str(), *hash)))
                } else {
                    None
                }
            })
        })
        .flatten()
        .collect();

    // Mirror Psalm's Reconciler::reconcileKeyedTypes contradiction reporting: an
    // assertion is eligible to report when it is referenced and active. Psalm gates
    // on `referenced_var_ids[$key]` and `active_new_types[$key][$offset]` — there is
    // no per-assertion-type allowlist.
    let mut active_assertion_offsets: BTreeMap<String, FxHashSet<usize>> = BTreeMap::new();
    for (var_name, var_assertions) in assertions {
        let offsets: FxHashSet<usize> = var_assertions
            .iter()
            .enumerate()
            .filter(|(_, assertion)| {
                !reconciled_assertions.contains(&(var_name.as_str(), assertion.to_hash()))
            })
            .map(|(offset, _)| offset)
            .collect();
        if !offsets.is_empty() {
            active_assertion_offsets.insert(var_name.clone(), offsets);
        }
    }

    if active_assertion_offsets.is_empty() {
        return;
    }

    let mut contradiction_context = context.clone();
    let mut changed_var_ids = FxHashSet::default();
    let inside_loop = contradiction_context.inside_loop;
    reconciler::reconcile_keyed_types(
        &reconciler::to_and_groups(assertions),
        &mut contradiction_context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        inside_loop,
        false,
        true,
        Some(&active_assertion_offsets),
    );
}

/// Merge variable assignments from branches back to parent context.
pub(crate) fn infer_condition_truthiness_from_clauses(
    entry_clauses: &[std::rc::Rc<Clause>],
    assertion_result: &assertion_finder::AssertionResult,
) -> Option<bool> {
    let true_branch_impossible =
        formula_contradicts_entry_clauses(entry_clauses, &assertion_result.if_true_clauses);
    let false_branch_impossible =
        formula_contradicts_entry_clauses(entry_clauses, &assertion_result.if_false_clauses);

    match (true_branch_impossible, false_branch_impossible) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => None,
    }
}

fn formula_contradicts_entry_clauses(
    entry_clauses: &[std::rc::Rc<Clause>],
    formula: &[Clause],
) -> bool {
    if entry_clauses.is_empty() || formula.is_empty() {
        return false;
    }

    let Ok(negated_formula) = negate_formula(formula.to_vec()) else {
        return false;
    };

    for negated_clause in negated_formula {
        if !negated_clause.reconcilable || negated_clause.wedge {
            continue;
        }

        for entry_clause in entry_clauses {
            if !entry_clause.reconcilable || entry_clause.wedge {
                continue;
            }

            let mut negated_contains_entry = true;
            for (key, entry_possibilities) in &entry_clause.possibilities {
                let Some(negated_possibilities) = negated_clause.possibilities.get(key) else {
                    negated_contains_entry = false;
                    break;
                };

                if negated_possibilities != entry_possibilities {
                    negated_contains_entry = false;
                    break;
                }

                if entry_possibilities.values().any(|assertion| {
                    matches!(assertion, Assertion::InArray(_) | Assertion::NotInArray(_))
                }) {
                    negated_contains_entry = false;
                    break;
                }
            }

            if negated_contains_entry {
                return true;
            }
        }
    }

    false
}

pub(crate) fn apply_correlated_equality_narrowing(
    analyzer: &StatementsAnalyzer<'_>,
    condition: &mago_syntax::ast::ast::expression::Expression<'_>,
    context: &mut BlockContext,
    condition_is_true: bool,
) {
    use mago_syntax::ast::ast::binary::BinaryOperator;
    use mago_syntax::ast::ast::expression::Expression;
    use mago_syntax::ast::ast::unary::UnaryPrefixOperator;

    match condition.unparenthesized() {
        Expression::UnaryPrefix(unary) if matches!(unary.operator, UnaryPrefixOperator::Not(_)) => {
            apply_correlated_equality_narrowing(
                analyzer,
                unary.operand,
                context,
                !condition_is_true,
            );
        }
        Expression::Parenthesized(parenthesized) => {
            apply_correlated_equality_narrowing(
                analyzer,
                parenthesized.expression,
                context,
                condition_is_true,
            );
        }
        Expression::Binary(binary) => {
            let implies_equality = match &binary.operator {
                BinaryOperator::Identical(_) => condition_is_true,
                BinaryOperator::NotIdentical(_) => !condition_is_true,
                _ => false,
            };

            if !implies_equality {
                return;
            }

            let Some(left_key) = expression_identifier::get_expression_var_key(binary.lhs) else {
                return;
            };
            let Some(right_key) = expression_identifier::get_expression_var_key(binary.rhs) else {
                return;
            };

            let Some(left_id) = analyzer.interner.find(&left_key) else {
                return;
            };
            let Some(right_id) = analyzer.interner.find(&right_key) else {
                return;
            };

            let Some(left_type) = context.locals.get(&left_id).cloned() else {
                return;
            };
            let Some(right_type) = context.locals.get(&right_id).cloned() else {
                return;
            };

            let Some(intersection) =
                assertion_reconciler::intersect_union_with_union(&left_type, &right_type)
            else {
                return;
            };

            context.locals.insert(left_id, intersection.clone());
            context.locals.insert(right_id, intersection);
        }
        _ => {}
    }
}
