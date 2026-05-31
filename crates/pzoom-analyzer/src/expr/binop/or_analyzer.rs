//! Or (||) operator analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::algebra::{Clause, get_truths_from_formula, negate_formula, simplify_cnf};
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

/// Analyze a logical OR expression (||, 'or').
///
/// The OR operator short-circuits: if the left side is truthy, the right side
/// is not evaluated. This analyzer handles type narrowing through negation.
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
        None,
    );

    // The right side executes only when the left side is falsy.
    let mut right_context = context.clone();
    right_context.inside_conditional = context.inside_conditional;
    let assertions = assertion_finder::get_assertions(analyzer, left, analysis_data);

    // Build the left-falsy narrowing the way Hakana does: negate the formula for
    // `left`, combine it with the clauses already known here, simplify, and extract
    // the truths. This folds in prior context knowledge and feeds a single reconcile.
    let left_cond_id = (left.start_offset() as u32, left.end_offset() as u32);
    let left_clauses =
        formula_generator::get_formula(left_cond_id, left_cond_id, left, analyzer, analysis_data, false)
            .unwrap_or_default();
    let negated_left_clauses = negate_formula(left_clauses).unwrap_or_default();
    let mut clauses_for_right_analysis: Vec<Clause> =
        context.clauses.iter().map(|c| (**c).clone()).collect();
    clauses_for_right_analysis.extend(negated_left_clauses.iter().cloned());
    let simplified_clauses = simplify_cnf(clauses_for_right_analysis.iter().collect());
    let mut left_referenced_var_ids = FxHashSet::<String>::default();
    let (negated_assertions, _active_negated_assertions) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(left_cond_id),
        &mut left_referenced_var_ids,
    );

    if !negated_assertions.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = right_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &negated_assertions,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            true,
            false,
            None,
        );
        promote_asserted_vars_to_assigned(analyzer, &assertions.if_false, &mut right_context);

        // Mirror Hakana: variables whose type the left-negation narrowed have their
        // now-stale clauses dropped before the right operand is analyzed, and the
        // removed (reconciled) clauses are recorded on both the right and the cond
        // context so the enclosing if/ternary body reconcile won't re-report them.
        if !changed_var_ids.is_empty() {
            let (right_kept, right_removed) = BlockContext::partition_reconciled_clause_refs(
                &right_context.clauses,
                &changed_var_ids,
                analyzer.interner,
            );
            right_context.clauses = right_kept;
            right_context
                .reconciled_expression_clauses
                .extend(right_removed);

            let (left_kept, left_removed) = BlockContext::partition_reconciled_clause_refs(
                &context.clauses,
                &changed_var_ids,
                analyzer.interner,
            );
            context.clauses = left_kept;
            context.reconciled_expression_clauses.extend(left_removed);
        }
    }
    // Snapshot assignment counts so we can identify variables the right operand
    // itself assigns (vs. those merely narrowed by the left-negation reconcile).
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
            None,
        );
    }

    // Second reconcile, matching hakana-core's or_analyzer and Psalm's OrAnalyzer:
    // reconcile the truths that hold when the right operand is truthy (the
    // negated-left clauses, minus any the right operand reassigned, combined with the
    // right operand's own formula) to surface redundant/paradoxical right operands.
    // The narrowing target is a throwaway clone — only the emitted issues matter.
    let right_cond_id = (right.start_offset() as u32, right.end_offset() as u32);
    let right_clauses =
        formula_generator::get_formula(right_cond_id, right_cond_id, right, analyzer, analysis_data, false)
            .unwrap_or_default();
    let right_assigned_var_ids: FxHashSet<pzoom_str::StrId> = right_context
        .assigned_var_ids
        .iter()
        .filter(|(var_id, count)| **count > pre_right_assigned.get(var_id).copied().unwrap_or(0))
        .map(|(var_id, _)| *var_id)
        .collect();
    let base_clauses: Vec<std::rc::Rc<Clause>> =
        simplified_clauses.iter().cloned().map(std::rc::Rc::new).collect();
    let base_clauses =
        BlockContext::remove_reconciled_clause_refs(&base_clauses, &right_assigned_var_ids, analyzer.interner).0;
    let mut combined_right_clauses: Vec<Clause> =
        base_clauses.iter().map(|c| (**c).clone()).collect();
    combined_right_clauses.extend(right_clauses);
    let combined_right_clauses = simplify_cnf(combined_right_clauses.iter().collect());
    let mut right_referenced_var_ids = FxHashSet::<String>::default();
    let (right_type_assertions, active_right_type_assertions) = get_truths_from_formula(
        combined_right_clauses.iter().collect(),
        Some(right_cond_id),
        &mut right_referenced_var_ids,
    );
    if !right_type_assertions.is_empty() {
        let mut right_issue_context = right_context.clone();
        let mut right_changed_var_ids = FxHashSet::default();
        let inside_loop = right_issue_context.inside_loop;
        reconciler::reconcile_keyed_types(
            &right_type_assertions,
            &mut right_issue_context,
            &mut right_changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            true,
            Some(&active_right_type_assertions),
        );
    }

    // The left-negation reconcile (Call A) narrows for the right operand; the result
    // "merge" below is a type-combine (combineUnionTypes), not a reconcile, matching
    // Psalm/Hakana. When this `||` is an if-condition, the body of `if ($a || $b)` is
    // the body of `if ($a || $b)` is entered when either operand is truthy, so each
    // variable's body type is the union of the left-truthy path (the un-narrowed
    // left/`context` type) and the left-falsy&right path (`right_context`), built with
    // a type-combine rather than a second reconcile (matching hakana-core's
    // or_analyzer). The outer context keeps its post-left (narrowing-free) locals as
    // the else/fallthrough base.
    if let Some(if_body_context) = context.if_body_context.clone() {
        let mut inner = if_body_context.borrow_mut();
        for (var_id, right_type) in &right_context.locals {
            match context.locals.get(var_id) {
                Some(left_type) => {
                    // Mirror Psalm's OrAnalyzer: a variable present in the base
                    // context gets the union of the left-truthy path (its base
                    // type) and the left-falsy&right path (`right_context`).
                    let combined =
                        pzoom_code_info::combine_union_types(right_type, left_type, false);
                    inner.locals.insert(*var_id, combined);
                }
                None => {
                    // The body of `if (A || B)` is entered when EITHER operand is
                    // truthy. A variable absent from the base context only has a
                    // `right_context` type because the left-falsy reconcile forced
                    // one (e.g. a property `$c->flag` set to `false` so the right
                    // operand could be analysed); asserting that in the body is
                    // wrong. Psalm's OrAnalyzer only merges right-context vars that
                    // already exist in the base context, so carry over just the
                    // ones the right operand actually assigned.
                    let right_count =
                        right_context.assigned_var_ids.get(var_id).copied().unwrap_or(0);
                    let pre_count = pre_right_assigned.get(var_id).copied().unwrap_or(0);
                    if right_count > pre_count {
                        inner.locals.insert(*var_id, right_type.clone());
                    }
                }
            }
        }
        inner
            .cond_referenced_var_ids
            .extend(right_context.cond_referenced_var_ids.iter().copied());
        for (var_id, count) in &right_context.assigned_var_ids {
            inner.assigned_var_ids.insert(*var_id, *count);
        }
    }

    // The right operand is only conditionally evaluated, so any variables it assigns
    // are *possibly* (re)defined after the `||`; combine them against the pre-`||`
    // type. Mirror Psalm's OrAnalyzer: only variables the right operand actually
    // assigned are merged back. A variable merely narrowed by the left-falsy
    // reconcile must not leak its left-falsy type onto the post-`||`/if-body base.
    context
        .cond_referenced_var_ids
        .extend(right_context.cond_referenced_var_ids.iter().copied());
    for (var_id, right_type) in &right_context.locals {
        let right_count = right_context.assigned_var_ids.get(var_id).copied().unwrap_or(0);
        let pre_count = pre_right_assigned.get(var_id).copied().unwrap_or(0);
        if right_count > pre_count {
            let combined = match context.locals.get(var_id) {
                Some(existing) => pzoom_code_info::combine_union_types(existing, right_type, false),
                None => right_type.clone(),
            };
            context.locals.insert(*var_id, combined);
            context.possibly_assigned_var_ids.insert(*var_id);
            if context.inside_conditional {
                context.assigned_var_ids.insert(*var_id, right_count);
            }
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
            // Keep both `$x` and `x` spellings in sync.
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
