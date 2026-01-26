//! While statement analyzer.

use mago_syntax::ast::ast::r#loop::r#while::While;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::stmt_analyzer::analyze_stmts;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze a while statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    while_stmt: &While<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the condition
    let _cond_pos = expr_analyzer::analyze(analyzer, while_stmt.condition, analysis_data, context);

    // Create a child context for the loop body
    let mut loop_context = context.child();
    loop_context.inside_loop = true;

    // Analyze the loop body using helper method
    let body_stmts = while_stmt.body.statements();
    analyze_stmts(analyzer, body_stmts, analysis_data, &mut loop_context)?;

    // Variables assigned in the loop body are "possibly assigned" in the parent
    for var_id in loop_context.assigned_var_ids.keys() {
        context.possibly_assigned_var_ids.insert(*var_id);
    }

    // The loop doesn't guarantee all paths exit (condition might be false initially)
    Ok(())
}
