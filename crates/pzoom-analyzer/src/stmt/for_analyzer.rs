//! For loop statement analyzer.

use mago_syntax::ast::ast::r#loop::r#for::{For, ForBody};
use pzoom_code_info::combine_union_types;
use rustc_hash::FxHashSet;

use crate::assertion_finder;
use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::reconciler;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt_analyzer;

/// Analyze a for loop statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    for_stmt: &For<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze initialization expressions
    for init_expr in for_stmt.initializations.iter() {
        let _ = expression_analyzer::analyze(analyzer, init_expr, analysis_data, context);
    }

    let pre_loop_assigned_var_ids = context.assigned_var_ids.clone();
    let pre_loop_possibly_assigned_var_ids = context.possibly_assigned_var_ids.clone();

    // Analyze the loop body
    let mut loop_context = context.child();
    loop_context.inside_loop = true;
    loop_context.inside_foreach = false;
    loop_context.assigned_var_ids = pre_loop_assigned_var_ids.clone();
    loop_context.possibly_assigned_var_ids = pre_loop_possibly_assigned_var_ids;

    broaden_loop_context_with_increment_effects(analyzer, for_stmt, &mut loop_context);

    for cond_expr in for_stmt.conditions.iter() {
        let _ = expression_analyzer::analyze(analyzer, cond_expr, analysis_data, &mut loop_context);
        let assertion_result = assertion_finder::get_assertions(analyzer, cond_expr, analysis_data);
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
    }

    match &for_stmt.body {
        ForBody::Statement(stmt) => {
            stmt_analyzer::analyze_stmt(analyzer, stmt, analysis_data, &mut loop_context)?;
        }
        ForBody::ColonDelimited(block) => {
            stmt_analyzer::analyze_stmts(
                analyzer,
                block.statements.as_slice(),
                analysis_data,
                &mut loop_context,
            )?;
        }
    };

    // Analyze increment expressions
    for inc_expr in for_stmt.increments.iter() {
        let _ = expression_analyzer::analyze(analyzer, inc_expr, analysis_data, &mut loop_context);
    }

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

    context.update_references_possibly_from_confusing_scope(&loop_context);

    Ok(())
}

fn broaden_loop_context_with_increment_effects(
    analyzer: &StatementsAnalyzer<'_>,
    for_stmt: &For<'_>,
    loop_context: &mut BlockContext,
) {
    if for_stmt.increments.is_empty() {
        return;
    }

    let mut speculative_context = loop_context.clone();
    let baseline_assigned = speculative_context.assigned_var_ids.clone();
    let mut speculative_analysis_data = FunctionAnalysisData::new();

    for inc_expr in for_stmt.increments.iter() {
        let _ = expression_analyzer::analyze(
            analyzer,
            inc_expr,
            &mut speculative_analysis_data,
            &mut speculative_context,
        );
    }

    for (var_id, speculative_type) in &speculative_context.locals {
        let baseline_count = baseline_assigned.get(var_id).copied().unwrap_or(0);
        let speculative_count = speculative_context
            .assigned_var_ids
            .get(var_id)
            .copied()
            .unwrap_or(0);
        if speculative_count <= baseline_count {
            continue;
        }

        if let Some(current_type) = loop_context.locals.get(var_id) {
            let combined = combine_union_types(current_type, speculative_type, false);
            loop_context.locals.insert(*var_id, combined);
        } else {
            loop_context.locals.insert(*var_id, speculative_type.clone());
        }
    }
}
