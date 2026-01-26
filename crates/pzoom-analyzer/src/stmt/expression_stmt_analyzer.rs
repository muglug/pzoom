//! Expression statement analyzer.

use mago_syntax::ast::ast::statement::ExpressionStatement;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an expression statement (a statement that is just an expression).
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    expr_stmt: &ExpressionStatement<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze the expression - the result type is discarded
    let _pos = expr_analyzer::analyze(analyzer, expr_stmt.expression, analysis_data, context);

    Ok(())
}
