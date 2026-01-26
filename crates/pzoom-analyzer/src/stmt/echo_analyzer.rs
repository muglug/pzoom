//! Echo statement analyzer.

use mago_syntax::ast::ast::echo::Echo;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an echo statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    echo: &Echo<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Analyze each expression being echoed
    for value in &echo.values {
        let _pos = expr_analyzer::analyze(analyzer, value, analysis_data, context);

        // TODO: Check that the type can be converted to string
        // (implements __toString or is scalar)
    }

    Ok(())
}
