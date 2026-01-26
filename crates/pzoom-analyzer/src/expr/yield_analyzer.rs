//! Yield expression analyzer.

use mago_syntax::ast::ast::r#yield::Yield;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze a yield expression.
///
/// yield produces a value from a generator function.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    yield_expr: &Yield<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    match yield_expr {
        Yield::Value(yield_value) => {
            // Analyze the value if present
            if let Some(value) = yield_value.value {
                let _value_pos = expr_analyzer::analyze(analyzer, value, analysis_data, context);
            }
        }
        Yield::Pair(yield_pair) => {
            // Analyze the key
            let _key_pos = expr_analyzer::analyze(analyzer, yield_pair.key, analysis_data, context);
            // Analyze the value
            let _value_pos = expr_analyzer::analyze(analyzer, yield_pair.value, analysis_data, context);
        }
        Yield::From(yield_from) => {
            // Analyze the delegated expression
            let _inner_pos = expr_analyzer::analyze(analyzer, yield_from.iterator, analysis_data, context);
        }
    }

    // yield returns the value sent to the generator via send()
    // Since we don't track this, return mixed
    analysis_data.set_expr_type(pos, TUnion::mixed());
}
