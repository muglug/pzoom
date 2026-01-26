//! Isset expression analyzer.

use mago_syntax::ast::ast::construct::{EmptyConstruct, IssetConstruct};

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expr_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;

/// Analyze an isset() expression.
///
/// isset() returns true if the variable exists and is not null.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    isset: &IssetConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Set context flag to suppress undefined variable warnings
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    // Analyze all values
    for value in isset.values.iter() {
        let _value_pos = expr_analyzer::analyze(analyzer, value, analysis_data, context);
    }

    context.inside_isset = was_inside_isset;

    // isset() always returns bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}

/// Analyze an empty() expression.
///
/// empty() returns true if the variable doesn't exist or is falsy.
pub fn analyze_empty(
    analyzer: &StatementsAnalyzer<'_>,
    empty: &EmptyConstruct<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Set context flag to suppress undefined variable warnings
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    // Analyze the value
    let _value_pos = expr_analyzer::analyze(analyzer, empty.value, analysis_data, context);

    context.inside_isset = was_inside_isset;

    // empty() always returns bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}
