//! For loop statement analyzer.

use mago_syntax::ast::ast::r#loop::r#for::{For, ForBody};

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
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
        let _ = expr_analyzer::analyze(analyzer, init_expr, analysis_data, context);
    }

    // Analyze condition expressions
    for cond_expr in for_stmt.conditions.iter() {
        let _ = expr_analyzer::analyze(analyzer, cond_expr, analysis_data, context);
    }

    // Analyze the loop body
    let mut loop_context = context.clone();
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
        let _ = expr_analyzer::analyze(analyzer, inc_expr, analysis_data, &mut loop_context);
    }

    Ok(())
}
