//! If/elseif/else statement analyzer (Psalm `IfElseAnalyzer` equivalent).

use mago_syntax::ast::ast::control_flow::r#if::If;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};
use crate::stmt::if_analyzer;

/// Analyze an if/elseif/else statement.
///
/// This is intentionally the statement-level entrypoint, mirroring Psalm's
/// `IfElseAnalyzer`, and delegates branch analysis to `if_analyzer`.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    if_stmt: &If<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    if_analyzer::analyze(analyzer, if_stmt, analysis_data, context)
}
