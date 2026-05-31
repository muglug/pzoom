//! Echo statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::echo::Echo;

use crate::context::BlockContext;
use crate::expr::echo_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an echo statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    echo: &Echo<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Mirrors Psalm `EchoAnalyzer`: echo writes to output and is impure in a
    // mutation-free context.
    let span = echo.span();
    echo_analyzer::emit_impure_output(
        analyzer,
        (span.start.offset, span.end.offset),
        analysis_data,
        "echo",
    );

    // Analyze each expression being echoed
    for value in &echo.values {
        let pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        if let Some(value_type) = analysis_data.get_expr_type(pos) {
            echo_analyzer::check_stringable(analyzer, &value_type, pos, analysis_data, "echo");
        }
    }

    Ok(())
}
