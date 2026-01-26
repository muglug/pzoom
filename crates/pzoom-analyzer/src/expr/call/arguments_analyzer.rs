//! Multiple arguments analyzer.
//!
//! This module analyzes all arguments in a function/method call.
//! Argument type verification against function parameters is handled
//! by the individual call analyzers (function_call_analyzer, instance_call_analyzer, etc.)
//! which have access to the function signature.

use mago_syntax::ast::ast::argument::ArgumentList;

use crate::context::BlockContext;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::StatementsAnalyzer;

use super::argument_analyzer;

/// Analyze all arguments in a function/method call.
///
/// This analyzes each argument expression to determine its type.
/// The specialized call analyzers (function_call_analyzer, instance_call_analyzer,
/// static_call_analyzer) are responsible for:
/// - Verifying argument count matches parameter count
/// - Handling default parameter values
/// - Handling variadic parameters
/// - Verifying argument types against parameter types
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    argument_list: &ArgumentList<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    for arg in argument_list.arguments.iter() {
        argument_analyzer::analyze(analyzer, arg, analysis_data, context);
    }
}
