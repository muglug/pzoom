//! Isset expression analyzer.

use mago_syntax::ast::ast::construct::IssetConstruct;
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::TUnion;

use crate::context::BlockContext;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

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
    for value in isset.values.iter() {
        let _value_pos = analyze_isset_var(analyzer, value, analysis_data, context);
    }

    // isset() always returns bool
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::bool()));
}

/// Psalm's `IssetAnalyzer::analyzeIssetVar`: analyze the inner expression
/// with `inside_isset` set, suppressing undefined-variable and
/// possibly-undefined-fetch reporting. Also used by the empty() analyzer.
pub(crate) fn analyze_isset_var(
    analyzer: &StatementsAnalyzer<'_>,
    value: &Expression<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Pos {
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    let value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);

    context.inside_isset = was_inside_isset;
    value_pos
}
