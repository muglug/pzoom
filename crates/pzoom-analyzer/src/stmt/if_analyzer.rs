//! If statement analyzer.
//!
//! This analyzer handles if/elseif/else statements with proper type algebra.
//! It tracks CNF formulas through nested conditions to enable advanced type narrowing.

use mago_syntax::ast::ast::control_flow::r#if::If;

use pzoom_code_info::algebra::{get_truths_from_formula, simplify_cnf, Clause};
use pzoom_code_info::combine_union_types;
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::stmt::control_analyzer::{self, ControlAction};
use crate::stmt_analyzer::analyze_stmts;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an if statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    if_stmt: &If<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition
    let _cond_pos = expr_analyzer::analyze(analyzer, if_stmt.condition, analysis_data, context);

    // Get type narrowing assertions from the condition using the assertion finder
    let assertion_result = assertion_finder::get_assertions(analyzer, if_stmt.condition);

    // Analyze the if branch with type narrowing
    let mut if_context = context.child();
    if_context.inside_conditional = true;

    // Combine parent clauses with the new if-true clauses
    let mut if_clauses: Vec<_> = context.clauses.iter().map(|c| (**c).clone()).collect();
    if_clauses.extend(assertion_result.if_true_clauses.clone());
    if_context.clauses = if_clauses.into_iter().map(std::rc::Rc::new).collect();

    // Apply assertions to if branch context using the reconciler
    apply_clauses_to_context(analyzer, &mut if_context, analysis_data);

    // Analyze the if body
    let if_stmts = if_stmt.body.statements();
    analyze_stmts(analyzer, if_stmts, analysis_data, &mut if_context)?;

    // Use control_analyzer to determine if the if branch exits
    let if_actions = control_analyzer::get_control_actions(if_stmts, analysis_data, &[], true);
    let if_exits = !if_actions.contains(&ControlAction::None);
    let mut all_branches_return = if_exits;

    // Build the negated clauses for the else branch
    // When entering else, we know the original condition is false
    let mut else_entry_clauses: Vec<_> = context.clauses.iter().map(|c| (**c).clone()).collect();
    else_entry_clauses.extend(assertion_result.if_false_clauses.clone());
    let else_entry_clauses: Vec<std::rc::Rc<Clause>> = else_entry_clauses.into_iter().map(std::rc::Rc::new).collect();

    // Analyze elseif branches
    let mut elseif_contexts = Vec::new();
    let mut current_else_clauses = else_entry_clauses.clone();

    for (elseif_cond, elseif_stmts) in if_stmt.body.else_if_clauses() {
        let mut elseif_context = context.child();
        elseif_context.inside_conditional = true;

        // Analyze elseif condition and get its assertions
        let _elseif_cond_pos =
            expr_analyzer::analyze(analyzer, elseif_cond, analysis_data, &mut elseif_context);
        let elseif_assertion_result = assertion_finder::get_assertions(analyzer, elseif_cond);

        // Elseif branch: current else clauses + elseif true clauses
        let mut elseif_clauses: Vec<_> = current_else_clauses.iter().map(|c| (**c).clone()).collect();
        elseif_clauses.extend(elseif_assertion_result.if_true_clauses.clone());
        elseif_context.clauses = elseif_clauses.into_iter().map(std::rc::Rc::new).collect();

        // Apply assertions from elseif condition
        apply_clauses_to_context(analyzer, &mut elseif_context, analysis_data);

        // Analyze elseif body
        analyze_stmts(analyzer, elseif_stmts, analysis_data, &mut elseif_context)?;

        // Use control_analyzer to determine if the elseif branch exits
        let elseif_actions = control_analyzer::get_control_actions(elseif_stmts, analysis_data, &[], true);
        let elseif_exits = !elseif_actions.contains(&ControlAction::None);
        all_branches_return = all_branches_return && elseif_exits;
        elseif_contexts.push(elseif_context);

        // Update current else clauses for the next elseif/else
        let new_clauses: Vec<std::rc::Rc<Clause>> = elseif_assertion_result.if_false_clauses.iter().cloned().map(std::rc::Rc::new).collect();
        current_else_clauses.extend(new_clauses);
    }

    // Analyze else branch
    let mut else_context = context.child();
    let has_else = if_stmt.body.has_else_clause();

    if let Some(else_stmts) = if_stmt.body.else_statements() {
        else_context.inside_conditional = true;
        else_context.clauses = current_else_clauses;

        // Apply negated assertions to else branch context using the reconciler
        apply_clauses_to_context(analyzer, &mut else_context, analysis_data);

        analyze_stmts(analyzer, else_stmts, analysis_data, &mut else_context)?;

        // Use control_analyzer to determine if the else branch exits
        let else_actions = control_analyzer::get_control_actions(else_stmts, analysis_data, &[], true);
        let else_exits = !else_actions.contains(&ControlAction::None);
        all_branches_return = all_branches_return && else_exits;
    } else {
        all_branches_return = false; // No else means not all paths return
    }

    // Merge contexts back - variables assigned in all branches are definitely assigned
    if has_else {
        merge_contexts(context, &if_context, &elseif_contexts, &else_context);
    }

    // Handle early returns: if the if-block exits and there's no else (and no elseifs),
    // code after the if statement only executes when the condition was false.
    // Add the negated clauses to the parent context.
    if !has_else && if_exits && elseif_contexts.is_empty() {
        // Add negated clauses to parent context
        let negated_clauses: Vec<std::rc::Rc<Clause>> = assertion_result.if_false_clauses.iter().cloned().map(std::rc::Rc::new).collect();
        context.clauses.extend(negated_clauses);

        // Apply the clauses to update variable types
        apply_clauses_to_context(analyzer, context, analysis_data);
    }

    // If all branches return, mark the context
    if all_branches_return {
        context.has_returned = true;
    }

    Ok(())
}

/// Apply clauses to a context by simplifying the CNF formula and extracting truths.
fn apply_clauses_to_context(
    analyzer: &StatementsAnalyzer<'_>,
    context: &mut BlockContext,
    analysis_data: &mut FunctionAnalysisData,
) {
    if context.clauses.is_empty() {
        return;
    }

    // Simplify the CNF formula
    let clause_refs: Vec<&Clause> = context.clauses.iter().map(|c| c.as_ref()).collect();
    let simplified = simplify_cnf(clause_refs);

    // Extract truths from the simplified formula
    let mut cond_referenced_var_ids = FxHashSet::default();
    let (truths, _active_truths) = get_truths_from_formula(
        simplified.iter().collect(),
        None,
        &mut cond_referenced_var_ids,
    );

    // Apply each truth to the context
    for (var_name, assertion_lists) in truths {
        let var_id = analyzer.interner.intern(&var_name);

        // Get the current type for this variable
        let existing_type = context
            .locals
            .get(&var_id)
            .cloned()
            .unwrap_or_else(pzoom_code_info::TUnion::mixed);

        // Apply each assertion in sequence using the reconciler
        let mut current_type = existing_type;
        for assertion_list in assertion_lists {
            for assertion in assertion_list {
                current_type = reconciler::reconcile(&assertion, &current_type, analyzer, analysis_data);
            }
        }

        // Update the context with the narrowed type
        context.locals.insert(var_id, current_type);
    }
}

/// Merge variable assignments from branches back to parent context.
fn merge_contexts(
    parent: &mut BlockContext,
    if_ctx: &BlockContext,
    elseif_ctxs: &[BlockContext],
    else_ctx: &BlockContext,
) {
    // Find variables assigned in ALL branches
    let mut all_contexts: Vec<&BlockContext> = vec![if_ctx, else_ctx];
    all_contexts.extend(elseif_ctxs.iter());

    // Get vars assigned in the first context
    let first_assigned: std::collections::HashSet<_> =
        if_ctx.assigned_var_ids.keys().copied().collect();

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

    // Collect all variables that exist in any branch
    let mut all_var_ids = std::collections::HashSet::new();
    for ctx in &all_contexts {
        for var_id in ctx.locals.keys() {
            all_var_ids.insert(*var_id);
        }
    }

    // Merge types - union of types from each branch where the variable exists
    for var_id in all_var_ids {
        let mut combined_type = None;

        for ctx in &all_contexts {
            if let Some(var_type) = ctx.locals.get(&var_id) {
                combined_type = Some(match combined_type {
                    Some(existing) => combine_union_types(&existing, var_type, false),
                    None => var_type.clone(),
                });
            }
        }

        if let Some(final_type) = combined_type {
            parent.locals.insert(var_id, final_type);
        }
    }
}
