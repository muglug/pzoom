//! Function and method call analyzer.
//!
//! This module dispatches to specialized analyzers in the `call/` submodule.

use mago_syntax::ast::ast::call::Call;

use crate::context::BlockContext;
use crate::expr::call::{function_call_analyzer, method_call_analyzer, static_call_analyzer};
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a function or method call expression.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    call: &Call<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match call {
        Call::Function(func_call) => {
            function_call_analyzer::analyze(analyzer, func_call, pos, analysis_data, context);
        }
        Call::Method(method_call) => {
            method_call_analyzer::analyze(analyzer, method_call, pos, analysis_data, context);
        }
        Call::NullSafeMethod(null_safe_call) => {
            method_call_analyzer::analyze_nullsafe(
                analyzer,
                null_safe_call,
                pos,
                analysis_data,
                context,
            );
        }
        Call::StaticMethod(static_call) => {
            static_call_analyzer::analyze(analyzer, static_call, pos, analysis_data, context);
        }
    }
}
