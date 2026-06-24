//! And (&&) operator analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::TUnion;
use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, get_truths_from_formula, simplify_cnf};
use rustc_hash::FxHashSet;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;
use std::rc::Rc;

/// Analyze a logical AND expression (&&, 'and').
///
/// The AND operator short-circuits: if the left side is falsy, the right side
/// is not evaluated. This analyzer handles type narrowing through the left side.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    right: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Faithful port of Psalm's AndAnalyzer. The left operand is analyzed in a
    // CLONE of the outer context; only the variables it genuinely *assigns* are
    // copied back into `context`. The right operand's context is then a fresh
    // clone of `context`, so a merely-*asserted* left variable keeps its pre-left
    // type and the left's assertions narrow it once (rather than being re-applied
    // to an already-narrowed type, which over-reported "$s is never int" for
    // `!is_int($s) && !is_bool($s) && !is_float($s)`).
    let pre_referenced_var_ids = context.cond_referenced_var_ids.clone();
    let pre_assigned_var_ids = context.assigned_var_ids.clone();

    let mut left_context = context.clone();
    left_context.cond_referenced_var_ids.clear();
    left_context.assigned_var_ids.clear();
    left_context.reconciled_expression_clauses.clear();

    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, &mut left_context);
    if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        left,
        left_pos,
        analysis_data,
        false,
        Some(&left_context),
    );

    let left_cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let left_clauses = formula_generator::get_formula(
        left_cond_id,
        left_cond_id,
        left,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();

    // Copy back into the outer context only the variables the left operand
    // assigned (Psalm: `if (isset($left_context->assigned_var_ids[$var_id]))`).
    for (var_id, var_type) in &left_context.locals {
        if left_context.assigned_var_ids.contains_key(var_id) {
            context.locals.insert(var_id.clone(), var_type.as_ref().clone());
        }
    }

    let mut left_referenced_var_ids = left_context.cond_referenced_var_ids.clone();
    context.cond_referenced_var_ids = pre_referenced_var_ids
        .union(&left_referenced_var_ids)
        .cloned()
        .collect();

    // Referenced-but-not-assigned vars gate the reconcile's active reporting set.
    let left_assigned_var_ids: FxHashSet<VarName> = left_context
        .assigned_var_ids
        .keys()
        .filter(|var_id| !pre_assigned_var_ids.contains_key(*var_id))
        .cloned()
        .collect();
    left_referenced_var_ids.retain(|var_id| !left_assigned_var_ids.contains(var_id));

    // Truths that hold when the left operand is true: left_context's accumulated
    // clauses plus the left formula, minus any the left analysis already reconciled.
    let mut context_clauses: Vec<Clause> =
        left_context.clauses.iter().map(|c| (**c).clone()).collect();
    context_clauses.extend(left_clauses.iter().cloned());
    if !left_context.reconciled_expression_clauses.is_empty() {
        let reconciled_hashes: FxHashSet<u32> = left_context
            .reconciled_expression_clauses
            .iter()
            .map(|c| c.hash)
            .collect();
        context_clauses.retain(|c| !reconciled_hashes.contains(&c.hash));
        if context_clauses.len() == 1
            && context_clauses[0].wedge
            && context_clauses[0].possibilities.is_empty()
        {
            context_clauses.clear();
        }
    }
    let simplified_clauses = simplify_cnf(context_clauses.iter().collect());
    let (left_type_assertions, mut active_left_assertions) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(left_cond_id),
        &mut left_referenced_var_ids,
    );
    // Psalm removes left-assigned vars from `$left_referenced_var_ids`, which gates
    // the reconcile's redundancy reporting, so `if (($c = f()) && ...)` does not
    // flag the always-truthy assigned `$c`. pzoom gates reporting on the active set,
    // so drop the assigned vars from it (as OrAnalyzer already does).
    for var_id in &left_assigned_var_ids {
        active_left_assertions.remove(var_id);
    }

    let mut changed_var_ids = FxHashSet::default();
    // While in an `&&`, scope is allowed to boil over so `if ($x && $x->foo())`
    // works: reconcile the left truths against a fresh clone of `context`.
    let mut right_context = if !left_type_assertions.is_empty() {
        let mut right_context = context.clone();
        let inside_loop = right_context.inside_loop;
        let left_span = left.span();
        let previous_reconcile_pos = analysis_data.current_reconcile_pos;
        analysis_data.current_reconcile_pos = Some((left_span.start.offset, left_span.end.offset));
        reconciler::reconcile_keyed_types(
            &left_type_assertions,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::All,
            Some(&active_left_assertions),
        );
        analysis_data.current_reconcile_pos = previous_reconcile_pos;
        right_context
    } else {
        left_context.clone()
    };

    // Partition `left_context.clauses + left_clauses` by the reconciled vars: the
    // kept clauses seed the right context; the removed ones are recorded so the
    // enclosing if body reconcile won't re-report assertions the `&&` evaluated.
    let combined_clause_rcs: Vec<std::rc::Rc<Clause>> = left_context
        .clauses
        .iter()
        .cloned()
        .chain(left_clauses.iter().cloned().map(std::rc::Rc::new))
        .collect();
    let (kept_clauses, reconciled_clauses) =
        BlockContext::partition_reconciled_clause_refs(&combined_clause_rcs, &changed_var_ids);
    right_context.clauses = kept_clauses;

    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);

    if context.inside_conditional || !matches!(right.unparenthesized(), Expression::Assignment(_)) {
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            right,
            right_pos,
            analysis_data,
            false,
            Some(&right_context),
        );
    }

    // Merge back, mirroring Psalm's AndAnalyzer tail (steps after analyzing the
    // right operand).
    context.cond_referenced_var_ids = right_context
        .cond_referenced_var_ids
        .union(&left_context.cond_referenced_var_ids)
        .cloned()
        .collect();

    // Psalm gates the following on `$context->inside_conditional`. pzoom's
    // IfConditionalAnalyzer scopes that flag to the sub-expression analysis and
    // restores it before this merge runs, so gating here would drop a condition's
    // assignments (`($pos = $str)` inside the `&&`) and the enclosing if could not
    // type the possibly-reassigned variable. Merge unconditionally.
    context.vars_possibly_in_scope = right_context
        .vars_possibly_in_scope
        .union(&left_context.vars_possibly_in_scope)
        .cloned()
        .collect();
    context.assigned_var_ids = left_context
        .assigned_var_ids
        .iter()
        .chain(right_context.assigned_var_ids.iter())
        .map(|(var_id, count)| (var_id.clone(), *count))
        .collect();

    // Psalm: `if ($context->if_body_context && !$context->inside_negation)`. pzoom
    // routes negated `&&` through De Morgan in the formula generator rather than an
    // `inside_negation` flag, so the flag check is unnecessary here.
    if let Some(if_body_context) = context.if_body_context.clone() {
        context.locals = right_context.locals.clone();
        let mut inner = if_body_context.borrow_mut();
        inner
            .locals
            .extend(context.locals.iter().map(|(k, v)| (k.clone(), v.clone())));
        inner
            .cond_referenced_var_ids
            .extend(context.cond_referenced_var_ids.iter().cloned());
        for (var_id, count) in &context.assigned_var_ids {
            inner.assigned_var_ids.insert(var_id.clone(), *count);
        }
        inner
            .reconciled_expression_clauses
            .extend(reconciled_clauses);
        inner
            .vars_possibly_in_scope
            .extend(context.vars_possibly_in_scope.iter().cloned());
    } else {
        context.locals = left_context.locals.clone();
    }

    // The result type is always bool
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::bool()));
}
