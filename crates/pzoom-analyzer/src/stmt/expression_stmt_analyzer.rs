//! Expression statement analyzer.

use mago_syntax::ast::ast::statement::ExpressionStatement;

use crate::context::BlockContext;
use crate::expression_analyzer;
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
    let pos = expression_analyzer::analyze(analyzer, expr_stmt.expression, analysis_data, context);

    // A statement-level expression of type `never` ends control flow.
    if analysis_data
        .get_expr_type(pos)
        .is_some_and(|t| t.is_nothing())
    {
        context.has_returned = true;
    }

    Ok(())
}
