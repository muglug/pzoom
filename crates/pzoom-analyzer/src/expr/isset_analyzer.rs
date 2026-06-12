//! Isset expression analyzer.

use mago_syntax::ast::ast::construct::{EmptyConstruct, IssetConstruct};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

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
    // Set context flag to suppress undefined variable warnings
    let was_inside_isset = context.inside_isset;
    context.inside_isset = true;

    // Analyze all values
    for value in isset.values.iter() {
        let _value_pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
    }

    context.inside_isset = was_inside_isset;

    // isset() always returns bool
    analysis_data.expr_types.insert(pos, Rc::new(TUnion::bool()));
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
    // Psalm's EmptyAnalyzer routes through IssetAnalyzer::analyzeIssetVar,
    // which sets inside_isset for the whole inner expression — `empty($x)` on
    // an undefined variable never reports. The boolean-refinement check below
    // still only applies to non-fetch expressions.
    let is_fetch_expression = matches!(
        empty.value.unparenthesized(),
        Expression::ArrayAccess(_) | Expression::Access(_)
    );

    let was_inside_isset = context.inside_isset;
    let was_inside_empty = context.inside_empty;
    context.inside_isset = true;
    context.inside_empty = true;

    // Analyze the value
    let value_pos = expression_analyzer::analyze(analyzer, empty.value, analysis_data, context);

    context.inside_isset = was_inside_isset;
    context.inside_empty = was_inside_empty;

    if !is_fetch_expression
        && analysis_data
        .expr_types.get(&value_pos).cloned()
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

    // Psalm's EmptyAnalyzer result typing: empty(always-truthy) is `false`,
    // empty(always-falsy) is `true` (docblock provenance preserved so the
    // surrounding condition reports the docblock-flavoured redundancy),
    // anything else is `bool`.
    let result_type = match analysis_data.expr_types.get(&value_pos).cloned() {
        Some(value_type) => {
            if value_type.is_always_truthy() && !value_type.possibly_undefined {
                let mut result = TUnion::new(TAtomic::TFalse);
                result.from_docblock = value_type.from_docblock;
                result
            } else if value_type.is_always_falsy() {
                let mut result = TUnion::new(TAtomic::TTrue);
                result.from_docblock = value_type.from_docblock;
                result
            } else {
                TUnion::bool()
            }
        }
        None => TUnion::bool(),
    };
    analysis_data.expr_types.insert(pos, Rc::new(result_type));
}
