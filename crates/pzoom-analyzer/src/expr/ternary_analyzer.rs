//! Ternary/conditional expression analyzer.
//!
//! Handles both full ternary (`$a ? $b : $c`) and elvis (`$a ?: $c`) operators.
//! Applies proper type narrowing based on the condition.

use std::collections::BTreeMap;
use std::rc::Rc;

use mago_span::HasSpan;
use mago_syntax::ast::ast::conditional::Conditional;
use mago_syntax::ast::ast::construct::Construct;
use rustc_hash::FxHashSet;

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::{Clause, get_truths_from_formula, negate_formula, simplify_cnf};
use pzoom_code_info::{Assertion, Issue, IssueKind, TUnion, combine_union_types};

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expr::binop::coalesce_analyzer;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::reconciler::{self, assertion_reconciler};
use crate::scope::IfScope;
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a conditional/ternary expression.
///
/// Handles both full ternary (`$a ? $b : $c`) and elvis (`$a ?: $c`) operators.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    cond: &Conditional<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    if let Some(if_expr) = cond.then {
        if let mago_syntax::ast::ast::expression::Expression::Construct(Construct::Isset(isset)) =
            cond.condition.unparenthesized()
        {
            if isset.values.len() == 1 {
                if let Some(isset_value) = isset.values.first() {
                    let isset_key = expression_identifier::get_expression_var_key(isset_value);
                    let if_key = expression_identifier::get_expression_var_key(if_expr);

                    if isset_key.is_some() && if_key.is_some() && isset_key == if_key {
                        coalesce_analyzer::analyze(
                            analyzer,
                            isset_value,
                            cond.r#else,
                            pos,
                            analysis_data,
                            context,
                        );
                        return;
                    }
                }
            }
        }
    }

    let mut if_scope = IfScope::default();

    // Analyze the condition expression. Psalm routes ternary conditions through
    // IfConditionalAnalyzer, which analyzes them with inside_conditional set —
    // the &&/|| analyzers gate their assigned-vars merge on it. Psalm also
    // gives the ternary condition its own if-body context, so a nested `&&`
    // boils its narrowing over into the ternary's true branch — never into an
    // enclosing if's body context.
    let was_inside_conditional = context.inside_conditional;
    let enclosing_if_body_context = context.if_body_context.take();
    context.inside_conditional = true;
    let cond_pos = expression_analyzer::analyze(analyzer, cond.condition, analysis_data, context);
    context.inside_conditional = was_inside_conditional;
    context.if_body_context = enclosing_if_body_context;

    // Psalm runs the ternary condition through IfConditionalAnalyzer, which flags an
    // always-truthy/always-falsy/risky condition (RedundantCondition,
    // TypeDoesNotContainType, RiskyTruthyFalsyComparison) just like a plain `if`.
    crate::stmt::if_conditional_analyzer::handle_paradoxical_condition(
        analyzer,
        cond.condition,
        cond_pos,
        analysis_data,
        true,
        Some(context),
    );

    // Get the condition type for later use
    let stmt_cond_type = analysis_data.expr_types.get(&cond_pos).cloned();

    // Get type narrowing assertions from the condition
    let assertion_result =
        assertion_finder::get_assertions(analyzer, cond.condition, analysis_data);
    emit_ternary_condition_paradox_if_needed(
        analyzer,
        cond.condition,
        &context.clauses,
        &assertion_result.if_true_clauses,
        analysis_data,
    );

    let cond_object_id = (
        cond.condition.start_offset() as u32,
        cond.condition.end_offset() as u32,
    );

    // Build clauses for the if branch
    let mut if_clauses = assertion_result.if_true_clauses.clone();

    // Limit clause count to prevent performance issues
    if if_clauses.len() > 200 {
        if_clauses = Vec::new();
    }

    // Combine with parent clauses
    let mut ternary_clauses: Vec<Clause> = if_clauses.clone();
    ternary_clauses.extend(
        context
            .clauses
            .iter()
            .map(|c| (**c).clone())
            .collect::<Vec<_>>(),
    );

    let simplified_ternary_clauses = simplify_cnf(ternary_clauses.iter().collect());

    // Get reconcilable truths from the combined clauses
    let mut cond_referenced_var_ids = FxHashSet::default();
    let (mut reconcilable_if_types, _active_if_types) = get_truths_from_formula(
        simplified_ternary_clauses.iter().collect(),
        Some(cond_object_id),
        &mut cond_referenced_var_ids,
    );
    merge_direct_assertions_into_reconciled_types(
        &mut reconcilable_if_types,
        &assertion_result.if_true,
    );

    if_scope.reasonable_clauses = simplified_ternary_clauses
        .into_iter()
        .map(Rc::new)
        .collect();

    // Negate the if clauses for the else branch
    if_scope.negated_clauses =
        negate_formula(if_clauses).unwrap_or_else(|_| assertion_result.if_false_clauses.clone());

    // Get negated types for the else branch
    let negated_clauses = simplify_cnf({
        let mut c: Vec<&Clause> = context.clauses.iter().map(|v| v.as_ref()).collect();
        c.extend(if_scope.negated_clauses.iter());
        c
    });

    let (mut new_negated_types, _) = get_truths_from_formula(
        negated_clauses.iter().collect(),
        None,
        &mut FxHashSet::default(),
    );
    merge_direct_assertions_into_reconciled_types(
        &mut new_negated_types,
        &assertion_result.if_false,
    );

    if_scope.negated_types = new_negated_types;

    // Create the if-branch context with type narrowing
    let mut if_context = context.child();
    if_context.inside_conditional = true;
    if_context.clauses = if_scope.reasonable_clauses.clone();

    // Apply type reconciliation to the if-branch context
    if !reconcilable_if_types.is_empty() {
        let mut if_changed_var_ids = FxHashSet::default();
        let inside_loop = if_context.inside_loop;
        // Psalm's reconciler issues point at the ternary condition.
        let previous_reconcile_pos = analysis_data.current_reconcile_pos;
        analysis_data.current_reconcile_pos = Some((
            cond.condition.start_offset() as u32,
            cond.condition.end_offset() as u32,
        ));
        reconciler::reconcile_keyed_types(
            &reconcilable_if_types,
            &mut if_context,
            &mut if_changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::Silent,
            None,
        );
        analysis_data.current_reconcile_pos = previous_reconcile_pos;
    }

    // Create the else-branch context (post-if context with negated types)
    let mut else_context = context.child();
    else_context.inside_conditional = true;
    let mut else_clauses: Vec<Clause> =
        else_context.clauses.iter().map(|c| (**c).clone()).collect();
    else_clauses.extend(if_scope.negated_clauses.clone());
    let simplified_else_clauses = simplify_cnf(else_clauses.iter().collect());
    else_context.clauses = simplified_else_clauses.into_iter().map(Rc::new).collect();

    // Analyze the branches
    let mut lhs_type: Option<TUnion> = None;

    // Check if there is an expression for the true case (full ternary vs elvis)
    if let Some(ref if_expr) = cond.then {
        // Full ternary: $a ? $b : $c
        let if_branch_pos =
            expression_analyzer::analyze(analyzer, if_expr, analysis_data, &mut if_context);

        // Merge cond_referenced_var_ids
        let mut new_referenced_var_ids = context.cond_referenced_var_ids.clone();
        new_referenced_var_ids.extend(if_context.cond_referenced_var_ids.clone());
        context.cond_referenced_var_ids = new_referenced_var_ids;

        if let Some(stmt_if_type) = analysis_data.expr_types.get(&if_branch_pos).cloned() {
            lhs_type = Some((*stmt_if_type).clone());
        }
    } else if let Some(cond_type) = &stmt_cond_type {
        // Elvis operator: $a ?: $c
        // The condition value itself becomes the true branch value
        // But we need to filter out falsy types since the condition was truthy
        let if_return_type_reconciled = assertion_reconciler::reconcile(
            &Assertion::Truthy,
            Some(&**cond_type),
            false, // possibly_undefined
            None,  // key
            analyzer,
            analysis_data,
            context.inside_loop,
            false, // negated
        );
        lhs_type = Some(if_return_type_reconciled);
    }

    // Apply negated types to else context
    if !if_scope.negated_types.is_empty() {
        let inside_loop = else_context.inside_loop;
        let mut changed_var_ids = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &if_scope.negated_types,
            &mut else_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            inside_loop,
            false,
            crate::reconciler::EmissionMode::Silent,
            None,
        );

        // Drop clauses invalidated by the narrowing (mirrors Hakana's load-bearing
        // remove_reconciled_clause_refs on the else context).
        let (new_clauses, _) =
            BlockContext::remove_reconciled_clause_refs(&else_context.clauses, &changed_var_ids);
        else_context.clauses = new_clauses;
    }

    // Analyze the else branch
    let else_pos =
        expression_analyzer::analyze(analyzer, cond.r#else, analysis_data, &mut else_context);

    // Get the else type
    let stmt_else_type = analysis_data.expr_types.get(&else_pos).cloned();

    // Merge variable assignments from both branches
    let assign_var_ifs = if_context.assigned_var_ids.clone();
    let assign_var_else = else_context.assigned_var_ids.clone();

    // Variables assigned in both branches
    let assign_all: FxHashSet<_> = assign_var_ifs
        .keys()
        .filter(|k| assign_var_else.contains_key(*k))
        .cloned()
        .collect();

    // If the same var was assigned in both branches, combine their types
    for var_id in &assign_all {
        if let (Some(if_type), Some(else_type)) = (
            if_context.locals.get(var_id),
            else_context.locals.get(var_id),
        ) {
            let combined = combine_union_types(if_type, else_type, false);
            context.locals.insert(var_id.clone(), combined);
        }
    }

    // Get variables redefined in each branch
    let mut removed_vars = FxHashSet::default();
    let redef_var_ifs: FxHashSet<_> = if_context
        .get_redefined_locals(&context.locals, false, &mut removed_vars)
        .into_keys()
        .collect();
    let redef_var_else: FxHashSet<_> = else_context
        .get_redefined_locals(&context.locals, false, &mut removed_vars)
        .into_keys()
        .collect();

    // Variables redefined in both branches
    let redef_all: FxHashSet<_> = redef_var_ifs
        .iter()
        .filter(|k| redef_var_else.contains(*k))
        .cloned()
        .collect();

    // Merge types for variables redefined in both branches
    for redef_var_id in &redef_all {
        if let (Some(if_type), Some(else_type)) = (
            if_context.locals.get(redef_var_id),
            else_context.locals.get(redef_var_id),
        ) {
            let combined = combine_union_types(if_type, else_type, false);
            context.locals.insert(redef_var_id.clone(), combined);
        }
    }

    // Handle variables redefined only in the if branch
    for redef_var_id in &redef_var_ifs {
        if !redef_all.contains(redef_var_id) && context.locals.contains_key(redef_var_id) {
            if let Some(if_type) = if_context.locals.get(redef_var_id) {
                let parent_type = context.locals.get(redef_var_id).unwrap();
                let combined = combine_union_types(parent_type, if_type, false);
                context.locals.insert(redef_var_id.clone(), combined);
            }
        }
    }

    // Handle variables redefined only in the else branch
    for redef_var_id in &redef_var_else {
        if !redef_all.contains(redef_var_id) && context.locals.contains_key(redef_var_id) {
            if let Some(else_type) = else_context.locals.get(redef_var_id) {
                let parent_type = context.locals.get(redef_var_id).unwrap();
                let combined = combine_union_types(parent_type, else_type, false);
                context.locals.insert(redef_var_id.clone(), combined);
            }
        }
    }

    // Merge cond_referenced_var_ids from else context
    context
        .cond_referenced_var_ids
        .extend(else_context.cond_referenced_var_ids);

    // Compute the result type
    let result_type = match (lhs_type, stmt_else_type) {
        (Some(lhs), Some(rhs)) => {
            // Check if condition is always truthy or always falsy
            if let Some(ref cond_type) = stmt_cond_type {
                if cond_type.is_always_falsy() {
                    // Condition always false, use else type
                    (*rhs).clone()
                } else if cond_type.is_always_truthy() {
                    // Condition always true, use if type
                    lhs
                } else if rhs.is_nothing() {
                    // Else branch is never type, use if type
                    lhs
                } else {
                    // Combine both types
                    combine_union_types(&lhs, &rhs, false)
                }
            } else if rhs.is_nothing() {
                lhs
            } else {
                combine_union_types(&lhs, &rhs, false)
            }
        }
        (Some(t), None) => t,
        (None, Some(t)) => (*t).clone(),
        (None, None) => TUnion::mixed(),
    };

    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}

fn emit_ternary_condition_paradox_if_needed(
    analyzer: &StatementsAnalyzer<'_>,
    condition: &mago_syntax::ast::ast::expression::Expression<'_>,
    entry_clauses: &[std::rc::Rc<Clause>],
    true_formula: &[Clause],
    analysis_data: &mut FunctionAnalysisData,
) {
    if !formula_contradicts_entry_clauses(entry_clauses, true_formula) {
        return;
    }

    let condition_pos = (
        condition.start_offset() as u32,
        condition.end_offset() as u32,
    );
    let (line, col) = analyzer.get_line_column(condition_pos.0);
    analysis_data.add_issue(Issue::new(
        IssueKind::ParadoxicalCondition,
        "Condition contradicts a previously-established condition".to_string(),
        analyzer.file_path,
        condition_pos.0,
        condition_pos.1,
        line,
        col,
    ));
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
            for (key, entry_possibilities) in entry_clause.possibilities.iter() {
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

fn merge_direct_assertions_into_reconciled_types(
    reconciled_types: &mut BTreeMap<VarName, Vec<Vec<Assertion>>>,
    direct_assertions: &BTreeMap<VarName, Vec<Vec<Assertion>>>,
) {
    // Both maps share Psalm's `$if_types` shape (AND groups of OR
    // alternatives), so the finder's groups append directly.
    for (var_name, groups) in direct_assertions {
        let entry = reconciled_types.entry(var_name.clone()).or_default();
        entry.extend(groups.iter().cloned());
    }
}
