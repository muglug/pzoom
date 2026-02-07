//! If statement analyzer.
//!
//! This analyzer handles if/elseif/else statements with proper type algebra.
//! It tracks CNF formulas through nested conditions to enable advanced type narrowing.

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
    Clause, ClauseKey, combine_ored_clauses, get_truths_from_formula, negate_formula, simplify_cnf,
};
use pzoom_code_info::{Assertion, TAtomic, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::expression_identifier;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::reconciler::assertion_reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::control_analyzer::{self, ControlAction};
use crate::stmt::if_conditional_analyzer;
use crate::stmt_analyzer::analyze_stmts;

/// Analyze an if statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    if_stmt: &If<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition
    let previous_inside_conditional = context.inside_conditional;
    context.inside_conditional = true;
    let cond_pos =
        expression_analyzer::analyze(analyzer, if_stmt.condition, analysis_data, context);

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
    context.inside_conditional = previous_inside_conditional;
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
            analysis_data,
        );
    }
    let if_conditional_id = (
        if_stmt.condition.start_offset() as u32,
        if_stmt.condition.end_offset() as u32,
    );

    // Analyze the if branch with type narrowing
    let mut if_context = context.child();
    seed_assignment_tracking(context, &mut if_context);

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

    // Analyze the if body
    let if_stmts = if_stmt.body.statements();
    analyze_stmts(analyzer, if_stmts, analysis_data, &mut if_context)?;

    // Use control_analyzer to determine if the if branch exits
    let if_actions = control_analyzer::get_control_actions(if_stmts, analysis_data, &[], true);
    let if_exits = !if_actions.contains(&ControlAction::None);
    let mut all_branches_return = if_exits;

    // Build the negated clauses for fallthrough branches (elseif/else).
    // When entering these branches, the original condition is false.
    let mut else_entry_clauses: Vec<_> = context.clauses.iter().map(|c| (**c).clone()).collect();
    else_entry_clauses.extend(assertion_result.if_false_clauses.clone());
    let else_entry_clauses: Vec<std::rc::Rc<Clause>> = else_entry_clauses
        .into_iter()
        .map(std::rc::Rc::new)
        .collect();
    let if_branch_condition_clauses = assertion_result.if_true_clauses.clone();
    let mut current_else_condition_clauses = assertion_result.if_false_clauses.clone();
    let mut current_else_context = context.child();
    seed_assignment_tracking(context, &mut current_else_context);
    current_else_context.clauses = else_entry_clauses.clone();
    apply_clauses_to_context(
        analyzer,
        &mut current_else_context,
        analysis_data,
        Some(if_conditional_id),
        false,
    );
    promote_asserted_vars_to_assigned(
        analyzer,
        &assertion_result.if_false,
        &mut current_else_context,
    );

    // Analyze elseif branches
    let mut elseif_contexts = Vec::new();
    let mut elseif_condition_clauses = Vec::new();
    let mut elseif_exit_states = Vec::new();

    for (elseif_cond, elseif_stmts) in if_stmt.body.else_if_clauses() {
        // Evaluate the elseif condition in the current fallthrough context.
        let mut elseif_condition_context = current_else_context.child();
        seed_assignment_tracking(&current_else_context, &mut elseif_condition_context);

        // Analyze elseif condition and get its assertions
        let previous_inside_conditional = elseif_condition_context.inside_conditional;
        elseif_condition_context.inside_conditional = true;
        let _elseif_cond_pos = expression_analyzer::analyze(
            analyzer,
            elseif_cond,
            analysis_data,
            &mut elseif_condition_context,
        );
        let elseif_assertion_result =
            assertion_finder::get_assertions(analyzer, elseif_cond, analysis_data);
        if !condition_has_assignments(elseif_cond)
            && !condition_contains_unsafe_empty_construct(elseif_cond)
        {
            if let Some(condition_is_always_truthy) = infer_condition_truthiness_from_clauses(
                &current_else_context.clauses,
                &elseif_assertion_result,
            ) {
                analysis_data.set_expr_type(
                    (
                        elseif_cond.start_offset() as u32,
                        elseif_cond.end_offset() as u32,
                    ),
                    if condition_is_always_truthy {
                        TUnion::new(TAtomic::TTrue)
                    } else {
                        TUnion::new(TAtomic::TFalse)
                    },
                );
            }
        }
        elseif_condition_context.inside_conditional = previous_inside_conditional;
        if_conditional_analyzer::handle_paradoxical_condition(
            analyzer,
            elseif_cond,
            (
                elseif_cond.start_offset() as u32,
                elseif_cond.end_offset() as u32,
            ),
            analysis_data,
            true,
            Some(&elseif_condition_context),
        );
        if !condition_has_assignments(elseif_cond) {
            emit_active_assertion_contradictions(
                analyzer,
                &current_else_context,
                &elseif_assertion_result.if_true,
                analysis_data,
            );
        }
        let elseif_conditional_id = (
            elseif_cond.start_offset() as u32,
            elseif_cond.end_offset() as u32,
        );
        let mut branch_condition_clauses = current_else_condition_clauses.clone();
        branch_condition_clauses.extend(elseif_assertion_result.if_true_clauses.clone());
        elseif_condition_clauses.push(branch_condition_clauses);

        // True elseif branch: condition side effects + elseif truthy assertions
        let mut elseif_context = elseif_condition_context.clone();
        let mut elseif_clauses: Vec<_> = elseif_context
            .clauses
            .iter()
            .map(|c| (**c).clone())
            .collect();
        elseif_clauses.extend(elseif_assertion_result.if_true_clauses.clone());
        elseif_context.clauses = elseif_clauses.into_iter().map(std::rc::Rc::new).collect();

        // Apply assertions from elseif condition
        apply_clauses_to_context(
            analyzer,
            &mut elseif_context,
            analysis_data,
            Some(elseif_conditional_id),
            false,
        );
        promote_asserted_vars_to_assigned(
            analyzer,
            &elseif_assertion_result.if_true,
            &mut elseif_context,
        );
        apply_correlated_equality_narrowing(analyzer, elseif_cond, &mut elseif_context, true);
        promote_guaranteed_true_condition_assignments(analyzer, elseif_cond, &mut elseif_context);

        // Analyze elseif body
        analyze_stmts(analyzer, elseif_stmts, analysis_data, &mut elseif_context)?;

        // Use control_analyzer to determine if the elseif branch exits
        let elseif_actions =
            control_analyzer::get_control_actions(elseif_stmts, analysis_data, &[], true);
        let elseif_exits = !elseif_actions.contains(&ControlAction::None);
        all_branches_return = all_branches_return && elseif_exits;
        elseif_exit_states.push(elseif_exits);
        elseif_contexts.push(elseif_context);

        // False elseif branch continues into the next elseif/else and keeps
        // condition side effects from evaluating this elseif condition.
        current_else_context = elseif_condition_context;
        let new_clauses: Vec<std::rc::Rc<Clause>> = elseif_assertion_result
            .if_false_clauses
            .iter()
            .cloned()
            .map(std::rc::Rc::new)
            .collect();
        current_else_context.clauses.extend(new_clauses);
        apply_clauses_to_context(
            analyzer,
            &mut current_else_context,
            analysis_data,
            Some(elseif_conditional_id),
            false,
        );
        promote_asserted_vars_to_assigned(
            analyzer,
            &elseif_assertion_result.if_false,
            &mut current_else_context,
        );
        current_else_condition_clauses.extend(elseif_assertion_result.if_false_clauses);
    }

    // Analyze else branch
    let mut else_context = current_else_context.clone();
    let mut else_exits = false;

    if let Some(else_stmts) = if_stmt.body.else_statements() {
        analyze_stmts(analyzer, else_stmts, analysis_data, &mut else_context)?;

        // Use control_analyzer to determine if the else branch exits
        let else_actions =
            control_analyzer::get_control_actions(else_stmts, analysis_data, &[], true);
        else_exits = !else_actions.contains(&ControlAction::None);
        all_branches_return = all_branches_return && else_exits;
    } else {
        // No explicit else: use the narrowed fallthrough context.
        all_branches_return = false; // No else means not all paths return
    }

    // Merge contexts back only from branches that can continue past the if.
    let mut continuing_contexts = Vec::new();
    let mut continuing_condition_sets = Vec::new();

    if !if_exits {
        continuing_contexts.push(&if_context);
        continuing_condition_sets.push(if_branch_condition_clauses);
    }

    for ((elseif_context, elseif_exits), condition_clauses) in elseif_contexts
        .iter()
        .zip(elseif_exit_states.iter())
        .zip(elseif_condition_clauses.into_iter())
    {
        if !*elseif_exits {
            continuing_contexts.push(elseif_context);
            continuing_condition_sets.push(condition_clauses);
        }
    }

    if !else_exits {
        continuing_contexts.push(&else_context);
    }

    context.update_references_possibly_from_confusing_scope(&if_context);
    for elseif_context in &elseif_contexts {
        context.update_references_possibly_from_confusing_scope(elseif_context);
    }
    context.update_references_possibly_from_confusing_scope(&else_context);

    if !continuing_contexts.is_empty() {
        merge_contexts(analyzer, context, &continuing_contexts);
    }

    if else_exits && !continuing_condition_sets.is_empty() {
        let conditional_object_id = (
            if_stmt.condition.start_offset() as u32,
            if_stmt.condition.end_offset() as u32,
        );
        let mut combined_condition_clauses = continuing_condition_sets.remove(0);

        for condition_set in continuing_condition_sets {
            if combined_condition_clauses.is_empty() || condition_set.is_empty() {
                combined_condition_clauses.clear();
                break;
            }

            combined_condition_clauses = combine_ored_clauses(
                combined_condition_clauses,
                condition_set,
                conditional_object_id,
            )
            .unwrap_or_default();
        }

        if !combined_condition_clauses.is_empty() {
            context
                .clauses
                .extend(combined_condition_clauses.into_iter().map(std::rc::Rc::new));
            apply_clauses_to_context(analyzer, context, analysis_data, None, false);
        }
    }

    // Handle early returns: if the if-block exits and there's no else (and no elseifs),
    // code after the if statement only executes when the condition was false.
    // Add the negated clauses to the parent context.
    if !if_stmt.body.has_else_clause() && if_exits && elseif_contexts.is_empty() {
        // Add negated clauses to parent context
        let negated_clauses: Vec<std::rc::Rc<Clause>> = assertion_result
            .if_false_clauses
            .iter()
            .cloned()
            .map(std::rc::Rc::new)
            .collect();
        context.clauses.extend(negated_clauses);

        // Apply the clauses to update variable types
        apply_clauses_to_context(analyzer, context, analysis_data, None, false);
        promote_asserted_vars_to_assigned(analyzer, &assertion_result.if_false, context);
        promote_guaranteed_false_condition_assignments(analyzer, if_stmt.condition, context);
    }

    // If all branches return, mark the context
    if all_branches_return {
        context.has_returned = true;
    }

    Ok(())
}

fn promote_asserted_vars_to_assigned(
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

fn promote_guaranteed_false_condition_assignments(
    analyzer: &StatementsAnalyzer<'_>,
    condition: &Expression<'_>,
    context: &mut BlockContext,
) {
    let guaranteed = collect_guaranteed_assignments(analyzer, condition);
    for var_id in guaranteed.when_false {
        if context.locals.contains_key(&var_id) {
            *context.assigned_var_ids.entry(var_id).or_insert(0) += 1;
            context.possibly_assigned_var_ids.remove(&var_id);
        }
    }
}

fn promote_guaranteed_true_condition_assignments(
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

fn seed_assignment_tracking(from: &BlockContext, to: &mut BlockContext) {
    to.assigned_var_ids = from.assigned_var_ids.clone();
    to.possibly_assigned_var_ids = from.possibly_assigned_var_ids.clone();
}

fn condition_has_assignments(expr: &Expression<'_>) -> bool {
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

fn condition_contains_unsafe_empty_construct(expr: &Expression<'_>) -> bool {
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
    let resolved_name = analyzer.interner.lookup(resolved_name_id);
    let function_name = resolved_name.as_ref();
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

        let treat_as_by_ref =
            by_ref_from_signature.unwrap_or(false) || is_preg_match_out_param(function_name, idx);
        if treat_as_by_ref {
            assigned.insert(analyzer.interner.intern(direct.name));
        }
    }

    assigned
}

fn is_preg_match_out_param(function_name: &str, param_idx: usize) -> bool {
    if param_idx != 2 {
        return false;
    }

    function_name.eq_ignore_ascii_case("preg_match")
        || function_name.eq_ignore_ascii_case("\\preg_match")
        || function_name.eq_ignore_ascii_case("preg_match_all")
        || function_name.eq_ignore_ascii_case("\\preg_match_all")
}

fn union_set(left: &FxHashSet<StrId>, right: &FxHashSet<StrId>) -> FxHashSet<StrId> {
    left.union(right).copied().collect()
}

fn intersect_set(left: &FxHashSet<StrId>, right: &FxHashSet<StrId>) -> FxHashSet<StrId> {
    left.intersection(right).copied().collect()
}

/// Apply clauses to a context by simplifying the CNF formula and extracting truths.
fn apply_clauses_to_context(
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

    let mut flattened_assertions: BTreeMap<String, Vec<pzoom_code_info::Assertion>> =
        BTreeMap::new();
    let mut flattened_active_assertion_offsets: BTreeMap<String, FxHashSet<usize>> =
        BTreeMap::new();

    for (var_name, assertion_lists) in truths {
        // Preserve OR groups from type algebra. Each inner list is OR-ed, and
        // multiple lists are applied conjunctively in order.
        let has_or_group = assertion_lists.iter().any(|assertion_list| assertion_list.len() > 1);
        if has_or_group {
            let var_id = analyzer.interner.intern(&var_name);
            let mut current_type = context
                .locals
                .get(&var_id)
                .cloned()
                .unwrap_or_else(pzoom_code_info::TUnion::mixed);

            for (assertion_list_index, assertion_list) in assertion_lists.into_iter().enumerate() {
                let is_active = active_truths
                    .get(&var_name)
                    .is_some_and(|offsets| offsets.contains(&assertion_list_index));
                let mut orred_outcome: Option<pzoom_code_info::TUnion> = None;

                for assertion in assertion_list {
                    let narrowed = assertion_reconciler::reconcile(
                        &assertion,
                        Some(&current_type),
                        false,
                        Some(&var_name),
                        analyzer,
                        analysis_data,
                        context.inside_loop,
                        emit_redundant_issues && is_active,
                    );

                    orred_outcome = Some(match orred_outcome {
                        Some(existing) => combine_union_types(&existing, &narrowed, false),
                        None => narrowed,
                    });
                }

                if let Some(outcome) = orred_outcome {
                    current_type = outcome;
                }
            }

            context.locals.insert(var_id, current_type);
        } else {
            let entry = flattened_assertions.entry(var_name.clone()).or_default();
            for (assertion_list_index, assertion_list) in assertion_lists.into_iter().enumerate() {
                let is_active = active_truths
                    .get(&var_name)
                    .is_some_and(|offsets| offsets.contains(&assertion_list_index));

                for assertion in assertion_list {
                    let next_offset = entry.len();
                    entry.push(assertion);

                    if emit_redundant_issues && is_active {
                        flattened_active_assertion_offsets
                            .entry(var_name.clone())
                            .or_default()
                            .insert(next_offset);
                    }
                }
            }
        }
    }

    let mut changed_var_ids = FxHashSet::default();
    reconciler::reconcile_keyed_types(
        &flattened_assertions,
        context,
        &mut changed_var_ids,
        analyzer,
        analysis_data,
        context.inside_loop,
        false,
        emit_redundant_issues,
        if emit_redundant_issues {
            Some(&flattened_active_assertion_offsets)
        } else {
            None
        },
    );
}

fn emit_active_assertion_contradictions(
    analyzer: &StatementsAnalyzer<'_>,
    context: &BlockContext,
    assertions: &BTreeMap<String, Vec<Assertion>>,
    analysis_data: &mut FunctionAnalysisData,
) {
    if assertions.is_empty() {
        return;
    }

    let mut filtered_assertions: BTreeMap<String, Vec<Assertion>> = BTreeMap::new();
    for (var_name, var_assertions) in assertions {
        let mut scoped_assertions = Vec::new();
        for assertion in var_assertions {
            if !matches!(
                assertion,
                Assertion::Truthy
                    | Assertion::Falsy
                    | Assertion::IsType(TAtomic::TNamedObject { .. })
                    | Assertion::IsNotType(TAtomic::TNamedObject { .. })
                    | Assertion::IsType(TAtomic::TInt)
                    | Assertion::IsNotType(TAtomic::TInt)
                    | Assertion::IsType(TAtomic::TIntRange { .. })
                    | Assertion::IsType(TAtomic::TFloat)
                    | Assertion::IsNotType(TAtomic::TFloat)
                    | Assertion::IsType(TAtomic::TString)
                    | Assertion::IsNotType(TAtomic::TString)
                    | Assertion::IsType(TAtomic::TArray { .. })
                    | Assertion::IsNotType(TAtomic::TArray { .. })
                    | Assertion::IsType(TAtomic::TNonEmptyArray { .. })
                    | Assertion::IsNotType(TAtomic::TNonEmptyArray { .. })
                    | Assertion::IsType(TAtomic::TList { .. })
                    | Assertion::IsNotType(TAtomic::TList { .. })
                    | Assertion::IsType(TAtomic::TNonEmptyList { .. })
                    | Assertion::IsNotType(TAtomic::TNonEmptyList { .. })
                    | Assertion::IsType(TAtomic::TKeyedArray { .. })
                    | Assertion::IsNotType(TAtomic::TKeyedArray { .. })
                    | Assertion::InArray(_)
                    | Assertion::NotInArray(_)
            ) {
                continue;
            }

            if matches!(
                assertion,
                Assertion::IsType(TAtomic::TNamedObject {
                    name: StrId::STATIC,
                    ..
                })
            ) {
                continue;
            }

            if matches!(assertion, Assertion::IsType(TAtomic::TString))
                && !context_has_in_array_clause_for_var(context, var_name)
            {
                continue;
            }

            if matches!(assertion, Assertion::Truthy | Assertion::Falsy)
                && (context.inside_loop || !context_has_clause_for_var(context, var_name))
            {
                continue;
            }

            if matches!(assertion, Assertion::Truthy | Assertion::Falsy)
                && (var_name.contains('[') || var_name.contains("->"))
            {
                continue;
            }

            scoped_assertions.push(assertion.clone());
        }

        if !scoped_assertions.is_empty() {
            filtered_assertions.insert(var_name.clone(), scoped_assertions);
        }
    }

    if filtered_assertions.is_empty() {
        return;
    }

    let mut active_assertion_offsets: BTreeMap<String, FxHashSet<usize>> = BTreeMap::new();
    for (var_name, var_assertions) in &filtered_assertions {
        if var_assertions.is_empty() {
            continue;
        }

        let mut offsets = FxHashSet::default();
        for offset in 0..var_assertions.len() {
            offsets.insert(offset);
        }

        active_assertion_offsets.insert(var_name.clone(), offsets);
    }

    if active_assertion_offsets.is_empty() {
        return;
    }

    let mut contradiction_context = context.clone();
    for (var_name, var_assertions) in &filtered_assertions {
        let has_named_object_assertion = var_assertions.iter().any(|assertion| {
            matches!(assertion, Assertion::IsType(TAtomic::TNamedObject { .. }))
        });

        if !has_named_object_assertion
            || !context_has_named_object_clause_for_var(context, var_name)
        {
            continue;
        }

        for var_id in [analyzer.interner.find(var_name), get_alternate_var_id(analyzer, var_name)]
            .into_iter()
            .flatten()
        {
            if let Some(var_type) = contradiction_context.locals.get_mut(&var_id) {
                var_type.from_docblock = false;
            }
        }
    }

    let mut changed_var_ids = FxHashSet::default();
    let inside_loop = contradiction_context.inside_loop;
    reconciler::reconcile_keyed_types(
        &filtered_assertions,
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
fn merge_contexts(
    analyzer: &StatementsAnalyzer<'_>,
    parent: &mut BlockContext,
    branch_contexts: &[&BlockContext],
) {
    if branch_contexts.is_empty() {
        return;
    }

    let parent_assigned_counts = parent.assigned_var_ids.clone();

    if branch_contexts.len() == 1 {
        parent.clauses = branch_contexts[0].clauses.clone();
    }

    // Find variables assigned in ALL branches
    let all_contexts = branch_contexts;

    // Get vars assigned in the first context
    let first_assigned: std::collections::HashSet<_> =
        all_contexts[0].assigned_var_ids.keys().copied().collect();

    // Find intersection with all other contexts
    let common_assigned: std::collections::HashSet<_> = first_assigned
        .into_iter()
        .filter(|var_id| {
            all_contexts
                .iter()
                .all(|ctx| ctx.assigned_var_ids.contains_key(var_id))
        })
        .collect();

    // Add commonly assigned vars to parent
    for var_id in &common_assigned {
        *parent.assigned_var_ids.entry(*var_id).or_insert(0) += 1;
    }

    // Variables only assigned in some branches are only possibly assigned afterwards.
    let mut all_assigned = std::collections::HashSet::new();
    for ctx in all_contexts {
        all_assigned.extend(ctx.assigned_var_ids.keys().copied());
    }

    for var_id in &all_assigned {
        // Reassignments of variables already in parent scope are not
        // "possibly new assignments" and should not affect undefined checks.
        if !common_assigned.contains(var_id) && !parent.locals.contains_key(var_id) {
            parent.possibly_assigned_var_ids.insert(*var_id);
        }
    }

    // Track branch-conditional reassignments of existing locals. This mirrors Psalm's
    // behavior of treating assertions on these vars as unstable after merge.
    for var_id in &all_assigned {
        let parent_assign_count = parent_assigned_counts.get(var_id).copied().unwrap_or(0);
        let mut branch_assign_counts = std::collections::HashSet::new();
        let mut saw_reassignment = false;

        for ctx in all_contexts {
            let branch_assign_count = ctx.assigned_var_ids.get(var_id).copied().unwrap_or(0);
            branch_assign_counts.insert(branch_assign_count);
            if branch_assign_count > parent_assign_count {
                saw_reassignment = true;
            }
        }

        if saw_reassignment && branch_assign_counts.len() > 1 {
            parent.possibly_assigned_var_ids.insert(*var_id);
        }
    }

    // Collect all variables that exist in any branch
    let mut all_var_ids = std::collections::HashSet::new();
    for ctx in all_contexts {
        for var_id in ctx.locals.keys() {
            all_var_ids.insert(*var_id);
        }
    }

    // Merge types - union of types from each branch where the variable exists
    for var_id in all_var_ids {
        let mut combined_type = None;
        let mut present_in_contexts = 0usize;

        for ctx in all_contexts {
            if let Some(var_type) = ctx.locals.get(&var_id) {
                present_in_contexts = present_in_contexts.saturating_add(1);
                combined_type = Some(match combined_type {
                    Some(existing) => combine_union_types(&existing, var_type, false),
                    None => var_type.clone(),
                });
            }
        }

        if let Some(final_type) = combined_type {
            let parent_had_local = parent.locals.contains_key(&var_id);
            let var_name = analyzer.interner.lookup(var_id);
            let is_path_local = var_name.contains('[') || var_name.contains("->");

            if is_path_local && present_in_contexts < all_contexts.len() {
                parent.locals.remove(&var_id);
                parent.assigned_var_ids.remove(&var_id);
                parent.possibly_assigned_var_ids.remove(&var_id);
                parent.class_string_origins.remove(&var_id);
                continue;
            }

            if present_in_contexts < all_contexts.len() && !parent_had_local {
                if var_name.contains("->") || var_name.contains('[') {
                    continue;
                }
            }

            let should_strip_mixed = !parent_had_local
                && present_in_contexts < all_contexts.len()
                && !var_name.contains('[')
                && !var_name.contains("->")
                && !var_name.contains("::");
            let final_type = if should_strip_mixed {
                strip_mixed_if_union_has_specific_types(&final_type)
            } else {
                final_type
            };
            parent.locals.insert(var_id, final_type);

            // Branch-local appearance/disappearance only implies "possibly assigned"
            // for variables that were not already in the parent scope.
            if present_in_contexts < all_contexts.len() && !parent_had_local {
                parent.possibly_assigned_var_ids.insert(var_id);
            }
        }
    }

    // Keep by-ref constraints from any continuing branch so follow-up assignments
    // can detect violations/conflicts after the merge point.
    for branch_context in all_contexts {
        for (var_id, constraints) in &branch_context.reference_constraints {
            let entry = parent.reference_constraints.entry(*var_id).or_default();
            for constraint in constraints {
                if !entry.contains(constraint) {
                    entry.push(constraint.clone());
                }
            }
        }
    }

    // If a variable was reassigned in any branch, prior path clauses for that
    // variable are no longer reliable after the merge point.
    let mut reassigned_var_ids = FxHashSet::default();
    for var_id in &all_assigned {
        let parent_assign_count = parent_assigned_counts.get(var_id).copied().unwrap_or(0);
        let mut branch_assign_counts = std::collections::HashSet::new();
        let mut saw_reassignment = false;

        for branch_context in all_contexts {
            let branch_assign_count = branch_context
                .assigned_var_ids
                .get(var_id)
                .copied()
                .unwrap_or(0);
            branch_assign_counts.insert(branch_assign_count);
            if branch_assign_count > parent_assign_count {
                saw_reassignment = true;
            }
        }

        if saw_reassignment && branch_assign_counts.len() > 1 {
            reassigned_var_ids.insert(*var_id);
        }
    }

    if !reassigned_var_ids.is_empty() {
        parent.clauses = BlockContext::remove_reconciled_clause_refs(
            &parent.clauses,
            &reassigned_var_ids,
            analyzer.interner,
        )
        .0;
    }
}

fn context_has_in_array_clause_for_var(context: &BlockContext, var_name: &str) -> bool {
    let clause_key = ClauseKey::Name(var_name.to_string());

    context.clauses.iter().any(|clause| {
        clause
            .possibilities
            .get(&clause_key)
            .is_some_and(|assertions| {
                assertions
                    .values()
                    .any(|assertion| matches!(assertion, Assertion::InArray(_)))
            })
    })
}

fn context_has_clause_for_var(context: &BlockContext, var_name: &str) -> bool {
    let clause_key = ClauseKey::Name(var_name.to_string());

    context
        .clauses
        .iter()
        .any(|clause| clause.possibilities.contains_key(&clause_key))
}

fn context_has_named_object_clause_for_var(context: &BlockContext, var_name: &str) -> bool {
    let clause_key = ClauseKey::Name(var_name.to_string());

    context.clauses.iter().any(|clause| {
        clause
            .possibilities
            .get(&clause_key)
            .is_some_and(|assertions| {
                assertions
                    .values()
                    .any(|assertion| matches!(assertion, Assertion::IsType(TAtomic::TNamedObject { .. })))
            })
    })
}

fn get_alternate_var_id(analyzer: &StatementsAnalyzer<'_>, var_name: &str) -> Option<StrId> {
    if var_name.contains('[') || var_name.contains("->") {
        return None;
    }

    if let Some(stripped) = var_name.strip_prefix('$') {
        analyzer.interner.find(stripped)
    } else {
        analyzer.interner.find(&format!("${}", var_name))
    }
}

fn infer_condition_truthiness_from_clauses(
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

fn strip_mixed_if_union_has_specific_types(union: &TUnion) -> TUnion {
    if !union.is_mixed() || union.types.len() <= 1 {
        return union.clone();
    }

    let filtered: Vec<_> = union
        .types
        .iter()
        .filter(|atomic| !matches!(atomic, TAtomic::TMixed | TAtomic::TNonEmptyMixed))
        .cloned()
        .collect();

    if filtered.is_empty() {
        union.clone()
    } else {
        let mut narrowed = TUnion::from_types(filtered);
        narrowed.from_docblock = union.from_docblock;
        narrowed.ignore_nullable_issues = union.ignore_nullable_issues;
        narrowed.ignore_falsable_issues = union.ignore_falsable_issues;
        narrowed
    }
}

fn apply_correlated_equality_narrowing(
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
