//! Isset expression analyzer.

use mago_syntax::ast::ast::construct::{EmptyConstruct, IssetConstruct};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expression_analyzer;
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
        let _value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
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
    // Psalm only applies isset-like suppression for fetch expressions.
    let use_isset_context = matches!(
        empty.value.unparenthesized(),
        Expression::ArrayAccess(_) | Expression::Access(_)
    );

    let was_inside_isset = context.inside_isset;
    if use_isset_context {
        context.inside_isset = true;
    }

    // Analyze the value
    let value_pos = expression_analyzer::analyze(analyzer, empty.value, analysis_data, context);

    if use_isset_context {
        context.inside_isset = was_inside_isset;
    }

    if !use_isset_context
        && analysis_data
        .get_expr_type(value_pos)
        .map(|value_type| {
            !value_type.types.is_empty()
                && value_type
                    .types
                    .iter()
                    .all(|atomic| matches!(atomic, TAtomic::TBool | TAtomic::TTrue | TAtomic::TFalse))
        })
        .unwrap_or(false)
    {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::InvalidArgument,
            "empty() cannot be used to refine boolean values",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // empty() always returns bool
    analysis_data.set_expr_type(pos, TUnion::bool());
}
