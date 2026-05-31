//! And (&&) operator analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::algebra::{Clause, get_truths_from_formula, simplify_cnf};
use pzoom_code_info::{Assertion, TUnion};
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::formula_generator;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler;
use crate::statements_analyzer::StatementsAnalyzer;
use crate::stmt::if_conditional_analyzer;

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
    // Analyze the left side
    let left_pos = expression_analyzer::analyze(analyzer, left, analysis_data, context);
    if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        left,
        left_pos,
        analysis_data,
        false,
        Some(context),
    );

    // The right side executes only when the left side is truthy.
    let mut right_context = context.clone();
    right_context.inside_conditional = context.inside_conditional;
    let assertions = assertion_finder::get_assertions(analyzer, left, analysis_data);

    // Build the left-truthy narrowing the way Hakana does: take the formula for
    // `left`, combine it with the clauses already known in this context, simplify,
    // and extract the truths. This folds in prior context knowledge that the raw
    // per-expression assertions miss, and feeds a single reconcile pass.
    let left_cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let left_clauses =
        formula_generator::get_formula(left_cond_id, left_cond_id, left, analyzer, analysis_data, false)
            .unwrap_or_default();
    let mut context_clauses: Vec<Clause> = context.clauses.iter().map(|c| (**c).clone()).collect();
    context_clauses.extend(left_clauses.iter().cloned());
    let simplified_clauses = simplify_cnf(context_clauses.iter().collect());
    let mut left_referenced_var_ids = FxHashSet::<String>::default();
    let (left_assertions, active_left_assertions) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(left_cond_id),
        &mut left_referenced_var_ids,
    );

    let mut reconciled_clauses: Vec<std::rc::Rc<Clause>> = Vec::new();
    let mut left_changed_var_ids = FxHashSet::default();
    if !left_assertions.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = right_context.inside_loop;
        // Mirror Hakana's and_analyzer: the left-truthy reconcile reports
        // contradictions/redundancies (can_report = true) at the `&&` itself. The
        // enclosing if then skips re-reporting these via reconciled_expression_clauses.
        reconciler::reconcile_keyed_types(
            &left_assertions,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            true,
            Some(&active_left_assertions),
        );
        promote_asserted_vars_to_assigned(analyzer, &assertions.if_true, &mut right_context);

        // Drop clauses invalidated by the left-truthy narrowing before the right
        // operand is analyzed (unchanged pzoom behavior over the existing clauses).
        if !changed_var_ids.is_empty() {
            right_context.clauses = BlockContext::remove_reconciled_clause_refs(
                &right_context.clauses,
                &changed_var_ids,
                analyzer.interner,
            )
            .0;

            // Separately, record the left-formula clauses the reconcile consumed so
            // the enclosing if/ternary body reconcile won't re-report assertions the
            // `&&` already evaluated (Hakana's reconciled_expression_clauses).
            let left_clause_rcs: Vec<std::rc::Rc<Clause>> =
                left_clauses.iter().cloned().map(std::rc::Rc::new).collect();
            reconciled_clauses = BlockContext::partition_reconciled_clause_refs(
                &left_clause_rcs,
                &changed_var_ids,
                analyzer.interner,
            )
            .1;
        }
        left_changed_var_ids = changed_var_ids;
    }
    // Snapshot assignment counts so we can identify variables the right operand
    // itself assigns (vs. those merely narrowed by the left-truthy reconcile).
    let pre_right_assigned = right_context.assigned_var_ids.clone();
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);

    if context.inside_conditional || !matches!(right.unparenthesized(), Expression::Assignment(_))
    {
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            right,
            right_pos,
            analysis_data,
            false,
            Some(&right_context),
        );
    }

    // Single-reconcile model (matching hakana-core's and_analyzer, which has one
    // reconcile_keyed_types call): only the left-truthy reconcile above narrows
    // types. When this `&&` is an if-condition, hand the right-truthy narrowing (and
    // right-operand assignments) up to the shared if_body_context so the if body
    // sees both operands narrowed.
    if let Some(if_body_context) = context.if_body_context.clone() {
        let mut inner = if_body_context.borrow_mut();
        inner.locals.extend(right_context.locals.clone());
        inner
            .cond_referenced_var_ids
            .extend(right_context.cond_referenced_var_ids.iter().copied());
        for (var_id, count) in &right_context.assigned_var_ids {
            inner.assigned_var_ids.insert(*var_id, *count);
        }
        // Mirror Hakana: record the clauses the `&&` already reconciled so the if
        // body reconcile skips them rather than re-reporting them as redundant.
        inner
            .reconciled_expression_clauses
            .extend(reconciled_clauses);
    }

    // The outer context keeps its post-left (narrowing-free) locals as the
    // fallthrough/else base (Hakana sets `context.locals = left_context.locals`).
    // The right operand is only conditionally evaluated, so any variables it assigns
    // are *possibly* (re)defined — propagate them combined against the pre-`&&` type
    // (mirroring Hakana's if_scope.possibly_redefined_vars), and carry over any
    // variables/expressions the right operand newly introduced (e.g. memoized
    // chained calls) so a fallthrough negation can still reason about them.
    context
        .cond_referenced_var_ids
        .extend(right_context.cond_referenced_var_ids.iter().copied());
    for (var_id, right_type) in &right_context.locals {
        let right_count = right_context.assigned_var_ids.get(var_id).copied().unwrap_or(0);
        let pre_count = pre_right_assigned.get(var_id).copied().unwrap_or(0);
        if right_count > pre_count {
            // The right operand is only conditionally evaluated, so a variable it
            // assigns is *possibly* (re)defined after the `&&`; combine it against the
            // pre-`&&` type. (Psalm types a statement-level `($x === null) && ($x = "")`
            // precisely via its from_stmt rewrite to `if ($x === null) { $x = ""; }`,
            // which pzoom cannot yet mirror without synthesizing an `if` AST node.)
            let combined = match context.locals.get(var_id) {
                Some(existing) => pzoom_code_info::combine_union_types(existing, right_type, false),
                None => right_type.clone(),
            };
            context.locals.insert(*var_id, combined);
            context.possibly_assigned_var_ids.insert(*var_id);
            if context.inside_conditional {
                context.assigned_var_ids.insert(*var_id, right_count);
            }
        } else if !context.locals.contains_key(var_id)
            && !left_changed_var_ids.contains(var_id)
        {
            // Carry over variables the right operand newly introduced, but not the
            // left-truthy *narrowing* of a lazily-resolved key (e.g. `$o->p` in
            // `is_string($o->p) && ...`): leaking that narrowed type into the
            // fallthrough context wrongly contradicts an alternative `||` branch.
            context.locals.insert(*var_id, right_type.clone());
        }
    }

    // The result type is always bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}

fn promote_asserted_vars_to_assigned(
    analyzer: &StatementsAnalyzer<'_>,
    assertions: &std::collections::BTreeMap<String, Vec<Assertion>>,
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

