//! Or (||) operator analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::binary::BinaryOperator;
use mago_syntax::ast::ast::expression::Expression;
use mago_syntax::ast::ast::variable::Variable;

use pzoom_code_info::VarName;
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
use std::rc::Rc;

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
    // When the left operand is itself an `||`, snapshot the pre-left context
    // (Psalm's $post_leaving_if_context) so conditionally-assigned vars can be
    // replayed for the right operand below.
    let left_is_or = matches!(
        left.unparenthesized(),
        Expression::Binary(binary) if matches!(binary.operator, BinaryOperator::Or(_) | BinaryOperator::LowOr(_))
    );
    let pre_left_context = left_is_or.then(|| context.clone());
    let pre_left_assigned = context.assigned_var_ids.clone();
    let pre_left_possibly_assigned = context.possibly_assigned_var_ids.clone();

    // Psalm's OrAnalyzer nulls if_body_context on both operand contexts
    // ($left_context->if_body_context = null): an `&&` nested inside an `||`
    // operand must not merge its narrowing into the enclosing if's shared body
    // context — only the `||`-level merge below reports there.
    let saved_if_body_context = context.if_body_context.take();

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

    // Psalm's IfAnalyzer::addConditionallyAssignedVarsToContext: when the ored
    // left operand assigned vars, replay its disjuncts in order against the
    // pre-left context — assignment effects interleaved with each disjunct's
    // if-false narrowing — and seed the right context with the assigned vars'
    // final types. A single CNF reconcile over the pre-assignment types cannot
    // express `!is_string($v) || ($v = X) === null || $v->foo()`.
    if let Some(pre_left_context) = pre_left_context {
        let mut left_assigned_var_ids: Vec<VarName> = context
            .assigned_var_ids
            .iter()
            .filter(|(var_id, count)| {
                **count > pre_left_assigned.get(*var_id).copied().unwrap_or(0)
            })
            .map(|(var_id, _)| var_id.clone())
            .collect();
        // A nested `||`'s merge records right-operand assignments only as
        // possibly assigned outside conditionals — count those too.
        for var_id in context
            .possibly_assigned_var_ids
            .difference(&pre_left_possibly_assigned)
        {
            if !left_assigned_var_ids.contains(var_id) {
                left_assigned_var_ids.push(var_id.clone());
            }
        }
        if !left_assigned_var_ids.is_empty() {
            add_conditionally_assigned_vars_to_context(
                analyzer,
                left,
                pre_left_context,
                &mut right_context,
                &left_assigned_var_ids,
                analysis_data,
            );
        }
    }

    // Build the left-falsy narrowing the way Hakana does: negate the formula for
    // `left`, combine it with the clauses already known here, simplify, and extract
    // the truths. This folds in prior context knowledge and feeds a single reconcile.
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
    let negated_left_clauses = negate_formula(left_clauses).unwrap_or_default();
    let mut clauses_for_right_analysis: Vec<Clause> =
        context.clauses.iter().map(|c| (**c).clone()).collect();
    clauses_for_right_analysis.extend(negated_left_clauses.iter().cloned());
    let simplified_clauses = simplify_cnf(clauses_for_right_analysis.iter().collect());
    let mut left_referenced_var_ids = FxHashSet::<VarName>::default();
    let (negated_assertions, mut active_negated_assertions) = get_truths_from_formula(
        simplified_clauses.iter().collect(),
        Some(left_cond_id),
        &mut left_referenced_var_ids,
    );

    // Psalm's AssignmentAnalyzer unsets assigned vars from
    // cond_referenced_var_ids, so a variable assigned inside the left operand
    // (`($v = expr) === null || ...`) never reports redundancy here.
    for (var_id, count) in &context.assigned_var_ids {
        if *count > pre_left_assigned.get(var_id).copied().unwrap_or(0) {
            active_negated_assertions.remove(var_id);
        }
    }

    // Psalm installs the simplified negation-conjoined formula as the right
    // context's clauses (`$right_context->clauses = $clauses_for_right_analysis`),
    // so surviving disjunctions like `!A ∨ B` resolve inside the right operand
    // (e.g. the third disjunct of `(A && !B) || (!A && B) || (A && ...B...)`).
    right_context.clauses = simplified_clauses
        .iter()
        .cloned()
        .map(std::rc::Rc::new)
        .collect();

    if !negated_assertions.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        let inside_loop = right_context.inside_loop;
        // Psalm's OrAnalyzer reconciles the negated left side with reporting
        // on (negated=!inside_negation), at CodeLocation($stmt->left): an
        // impossible/redundant left-negation surfaces here — e.g. the lone
        // "Docblock-defined type int for $x is never null" survivor when a
        // nested `||` chain compares never-null vars to null. Same-position
        // duplicates collapse through the dupe key.
        let left_span = left.span();
        let previous_reconcile_pos = analysis_data.current_reconcile_pos;
        analysis_data.current_reconcile_pos = Some((left_span.start.offset, left_span.end.offset));
        reconciler::reconcile_keyed_types(
            &negated_assertions,
            &mut right_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            true,
            crate::reconciler::EmissionMode::All,
            Some(&active_negated_assertions),
        );
        analysis_data.current_reconcile_pos = previous_reconcile_pos;
        promote_asserted_vars_to_assigned(&assertions.if_false, &mut right_context);

        // Mirror Hakana: variables whose type the left-negation narrowed have their
        // now-stale clauses dropped before the right operand is analyzed, and the
        // removed (reconciled) clauses are recorded on both the right and the cond
        // context so the enclosing if/ternary body reconcile won't re-report them.
        if !changed_var_ids.is_empty() {
            let (right_kept, right_removed) = BlockContext::partition_reconciled_clause_refs(
                &right_context.clauses,
                &changed_var_ids,
            );
            right_context.clauses = right_kept;
            right_context
                .reconciled_expression_clauses
                .extend(right_removed);

            let (left_kept, left_removed) =
                BlockContext::partition_reconciled_clause_refs(&context.clauses, &changed_var_ids);
            context.clauses = left_kept;
            context.reconciled_expression_clauses.extend(left_removed);
        }
    }
    // Snapshot assignment counts so we can identify variables the right operand
    // itself assigns (vs. those merely narrowed by the left-negation reconcile).
    let pre_right_assigned = right_context.assigned_var_ids.clone();
    let right_pos =
        expression_analyzer::analyze(analyzer, right, analysis_data, &mut right_context);
    if context.inside_conditional || !matches!(right.unparenthesized(), Expression::Assignment(_)) {
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
    let right_clauses = formula_generator::get_formula(
        right_cond_id,
        right_cond_id,
        right,
        analyzer,
        analysis_data,
        false,
    )
    .unwrap_or_default();
    let right_assigned_var_ids: FxHashSet<VarName> = right_context
        .assigned_var_ids
        .iter()
        .filter(|(var_id, count)| **count > pre_right_assigned.get(*var_id).copied().unwrap_or(0))
        .map(|(var_id, _)| var_id.clone())
        .collect();
    let base_clauses: Vec<std::rc::Rc<Clause>> = simplified_clauses
        .iter()
        .cloned()
        .map(std::rc::Rc::new)
        .collect();
    let base_clauses =
        BlockContext::remove_reconciled_clause_refs(&base_clauses, &right_assigned_var_ids).0;
    let mut combined_right_clauses: Vec<Clause> =
        base_clauses.iter().map(|c| (**c).clone()).collect();
    combined_right_clauses.extend(right_clauses);
    let combined_right_clauses = simplify_cnf(combined_right_clauses.iter().collect());
    let mut right_referenced_var_ids = FxHashSet::<VarName>::default();
    let (right_type_assertions, mut active_right_type_assertions) = get_truths_from_formula(
        combined_right_clauses.iter().collect(),
        Some(right_cond_id),
        &mut right_referenced_var_ids,
    );
    // Psalm gates this reconcile's reporting on $right_referenced_var_ids —
    // and its AssignmentAnalyzer unsets assigned vars from
    // cond_referenced_var_ids, so a variable assigned in the right operand
    // (`A || $x = expr`) never reports redundancy here. pzoom doesn't track
    // cond-referenced ids, so drop right-assigned vars from the active set
    // (the reconcile still narrows; only reporting is silenced, like Psalm
    // passing a null code location).
    for assigned_var_id in &right_assigned_var_ids {
        active_right_type_assertions.remove(assigned_var_id);
    }
    if !right_type_assertions.is_empty() {
        let mut right_issue_context = right_context.clone();
        let mut right_changed_var_ids = FxHashSet::default();
        let inside_loop = right_issue_context.inside_loop;
        // Psalm's OrAnalyzer reconciles with CodeLocation($stmt->right): the
        // reported position is the right operand — letting the dupe key
        // collapse this against the assertion finder's per-comparison issue.
        let right_span = right.span();
        let previous_reconcile_pos = analysis_data.current_reconcile_pos;
        analysis_data.current_reconcile_pos =
            Some((right_span.start.offset, right_span.end.offset));
        reconciler::reconcile_keyed_types(
            &right_type_assertions,
            &mut right_issue_context,
            &mut right_changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::All,
            Some(&active_right_type_assertions),
        );
        analysis_data.current_reconcile_pos = previous_reconcile_pos;
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
    context.if_body_context = saved_if_body_context;
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
                    inner.locals.insert(var_id.clone(), combined);
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
                    let right_count = right_context
                        .assigned_var_ids
                        .get(var_id)
                        .copied()
                        .unwrap_or(0);
                    let pre_count = pre_right_assigned.get(var_id).copied().unwrap_or(0);
                    if right_count > pre_count {
                        inner.locals.insert(var_id.clone(), right_type.clone());
                    }
                }
            }
        }
        inner
            .cond_referenced_var_ids
            .extend(right_context.cond_referenced_var_ids.iter().cloned());
        for (var_id, count) in &right_context.assigned_var_ids {
            inner.assigned_var_ids.insert(var_id.clone(), *count);
        }
    }

    // The right operand only runs when the left is falsy, so after the `||`
    // each variable may carry either its left-truthy type or its
    // left-falsy-and-right type. Psalm's OrAnalyzer combines EVERY right
    // context var that already exists in the outer context — including ones
    // merely narrowed by the left-falsy reconcile, so `$s !== 'x' || f()`
    // leaks 'x' back into $s's post-`||` union. Vars the right side created
    // from nothing are not added.
    context
        .cond_referenced_var_ids
        .extend(right_context.cond_referenced_var_ids.iter().cloned());
    // Psalm skips the type merge when the right operand exits
    // (`$cond || die()`): the fall-through path never saw the right side.
    let right_exits = matches!(
        right.unparenthesized(),
        Expression::Construct(construct) if matches!(
            construct,
            mago_syntax::ast::ast::construct::Construct::Exit(_)
                | mago_syntax::ast::ast::construct::Construct::Die(_)
        )
    );
    for (var_id, right_type) in &right_context.locals {
        if !right_exits && let Some(existing) = context.locals.get(var_id) {
            let combined = pzoom_code_info::combine_union_types(existing, right_type, false);
            context.locals.insert(var_id.clone(), combined);
        }
        let right_count = right_context
            .assigned_var_ids
            .get(var_id)
            .copied()
            .unwrap_or(0);
        let pre_count = pre_right_assigned.get(var_id).copied().unwrap_or(0);
        if right_count > pre_count {
            // A var the right operand created (`!$b || !($a = $b->c)`) must
            // still reach the fall-through so the else branch reconciles the
            // real assigned type rather than minting one from the assertion.
            if !context.locals.contains_key(var_id) {
                context.locals.insert(var_id.clone(), right_type.clone());
            }
            context.possibly_assigned_var_ids.insert(var_id.clone());
            // Psalm merges the right context's assigned ids back
            // unconditionally (`$context->assigned_var_ids = [...$context...,
            // ...$right_context->assigned_var_ids]`).
            context.assigned_var_ids.insert(var_id.clone(), right_count);
        }
    }

    // The result type is always bool
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::bool()));
}

/// Mini-port of Psalm's `IfAnalyzer::addConditionallyAssignedVarsToContext`:
/// replay each definitely-evaluated ored subexpression in order against a
/// pre-left clone — applying its assignments' recorded types, then reconciling
/// its if-false assertions — and copy the assigned vars' final types into the
/// right context. (Psalm re-analyzes `assert(!expr)` per disjunct under a
/// recording issue buffer; using the recorded assignment types avoids the
/// re-analysis while producing the same narrow→assign→narrow sequencing.)
fn add_conditionally_assigned_vars_to_context(
    analyzer: &StatementsAnalyzer<'_>,
    left: &Expression<'_>,
    mut replay_context: BlockContext,
    right_context: &mut BlockContext,
    left_assigned_var_ids: &[VarName],
    analysis_data: &mut FunctionAnalysisData,
) {
    let mut ored_exprs = Vec::new();
    flatten_ored_expressions(left, &mut ored_exprs);

    for expr in ored_exprs {
        apply_recorded_assignments(expr, analysis_data, &mut replay_context);

        let assertions = assertion_finder::get_assertions(analyzer, expr, analysis_data);
        if !assertions.if_false.is_empty() {
            let mut changed_var_ids = FxHashSet::default();
            let inside_loop = replay_context.inside_loop;
            reconciler::reconcile_keyed_types(
                &assertions.if_false,
                &mut replay_context,
                &mut changed_var_ids,
                analyzer,
                analysis_data,
                inside_loop,
                false,
                crate::reconciler::EmissionMode::Silent,
                None,
            );
        }
    }

    for var_id in left_assigned_var_ids {
        if let Some(final_type) = replay_context.locals.get(var_id) {
            right_context
                .locals
                .insert(var_id.clone(), final_type.clone());
        }
    }
}

/// Flatten nested `||`/`or` into the ordered list of disjuncts (Psalm's
/// getDefinitelyEvaluatedOredExpressions).
pub(crate) fn flatten_ored_expressions<'a, 'arena>(
    expr: &'a Expression<'arena>,
    out: &mut Vec<&'a Expression<'arena>>,
) {
    if let Expression::Binary(binary) = expr.unparenthesized()
        && matches!(
            binary.operator,
            BinaryOperator::Or(_) | BinaryOperator::LowOr(_)
        )
    {
        flatten_ored_expressions(binary.lhs, out);
        flatten_ored_expressions(binary.rhs, out);
        return;
    }
    out.push(expr);
}

/// Re-apply `$v = ...` assignments inside a disjunct using the types recorded
/// during the left operand's analysis.
pub(crate) fn apply_recorded_assignments(
    expr: &Expression<'_>,
    analysis_data: &FunctionAnalysisData,
    replay_context: &mut BlockContext,
) {
    match expr.unparenthesized() {
        Expression::Assignment(assignment) => {
            apply_recorded_assignments(assignment.rhs, analysis_data, replay_context);
            if let Expression::Variable(Variable::Direct(direct)) = assignment.lhs.unparenthesized()
            {
                let span = assignment.rhs.span();
                if let Some(assigned_type) = analysis_data
                    .expr_types
                    .get(&(span.start.offset, span.end.offset))
                    .cloned()
                {
                    replay_context
                        .set_var_type(VarName::new(direct.name), assigned_type.as_ref().clone());
                }
            }
        }
        Expression::Binary(binary) => {
            apply_recorded_assignments(binary.lhs, analysis_data, replay_context);
            apply_recorded_assignments(binary.rhs, analysis_data, replay_context);
        }
        Expression::UnaryPrefix(unary) => {
            apply_recorded_assignments(unary.operand, analysis_data, replay_context);
        }
        _ => {}
    }
}

fn promote_asserted_vars_to_assigned(
    assertions: &std::collections::BTreeMap<VarName, Vec<Vec<Assertion>>>,
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
            let with_dollar = VarName::from(format!("${var_name}"));
            if context.locals.contains_key(&with_dollar) {
                *context
                    .assigned_var_ids
                    .entry(with_dollar.clone())
                    .or_insert(0) += 1;
                context.possibly_assigned_var_ids.remove(&with_dollar);
            }
        }

        for candidate in candidates {
            let candidate = VarName::new(candidate);
            if context.locals.contains_key(&candidate) {
                *context
                    .assigned_var_ids
                    .entry(candidate.clone())
                    .or_insert(0) += 1;
                context.possibly_assigned_var_ids.remove(&candidate);
            }
        }
    }
}
