//! While statement analyzer.

use mago_syntax::ast::ast::r#loop::r#while::While;
use pzoom_code_info::combine_union_types;
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer::analyze_stmts;

/// Analyze a while statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    while_stmt: &While<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition
    let was_inside_loop = context.inside_loop;
    context.inside_loop = true;
    let _cond_pos =
        expression_analyzer::analyze(analyzer, while_stmt.condition, analysis_data, context);
    context.inside_loop = was_inside_loop;
    let assertion_result =
        assertion_finder::get_assertions(analyzer, while_stmt.condition, analysis_data);
    let pre_loop_assigned_var_ids = context.assigned_var_ids.clone();
    let pre_loop_possibly_assigned_var_ids = context.possibly_assigned_var_ids.clone();

    // Create a child context for the loop body
    let mut loop_context = context.child();
    loop_context.inside_loop = true;
    loop_context.inside_foreach = false;
    loop_context.assigned_var_ids = pre_loop_assigned_var_ids.clone();
    loop_context.possibly_assigned_var_ids = pre_loop_possibly_assigned_var_ids;
    if !assertion_result.if_true.is_empty() {
        let mut changed_var_ids = FxHashSet::default();
        reconciler::reconcile_keyed_types(
            &assertion_result.if_true,
            &mut loop_context,
            &mut changed_var_ids,
            analyzer,
            analysis_data,
            true,
            false,
            false,
            None,
        );
    }

    // Analyze the loop body using helper method
    let body_stmts = while_stmt.body.statements();
    analyze_stmts(analyzer, body_stmts, analysis_data, &mut loop_context)?;

    // Variables assigned in the loop body are "possibly assigned" in the parent
    for (var_id, loop_assigned_count) in &loop_context.assigned_var_ids {
        let pre_loop_count = pre_loop_assigned_var_ids.get(var_id).copied().unwrap_or(0);
        if *loop_assigned_count <= pre_loop_count {
            continue;
        }

        let parent_had_local = context.locals.contains_key(var_id);
        if !parent_had_local {
            context.possibly_assigned_var_ids.insert(*var_id);
        }

        if let Some(loop_type) = loop_context.locals.get(var_id) {
            if let Some(parent_type) = context.locals.get(var_id) {
                let combined = combine_union_types(parent_type, loop_type, false);
                context.locals.insert(*var_id, combined);
            } else {
                context.locals.insert(*var_id, loop_type.clone());
            }
        }
    }

    let reassigned_var_ids: FxHashSet<_> = loop_context
        .assigned_var_ids
        .iter()
        .filter_map(|(var_id, loop_assigned_count)| {
            let pre_loop_count = pre_loop_assigned_var_ids.get(var_id).copied().unwrap_or(0);
            (loop_assigned_count > &pre_loop_count).then_some(*var_id)
        })
        .collect();

    if !reassigned_var_ids.is_empty() {
        context.clauses = BlockContext::remove_reconciled_clause_refs(
            &context.clauses,
            &reassigned_var_ids,
            analyzer.interner,
        )
        .0;
    }

    context.update_references_possibly_from_confusing_scope(&loop_context);

    // The loop doesn't guarantee all paths exit (condition might be false initially)
    Ok(())
}
