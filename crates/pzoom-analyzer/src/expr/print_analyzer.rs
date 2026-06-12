//! Print expression analyzer.

use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind, TAtomic, TUnion};

use crate::context::BlockContext;
use crate::expr::output_constructs;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::StatementsAnalyzer;
use std::rc::Rc;

/// Analyze a print expression.
///
/// print outputs a single expression and returns 1.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    expr: &Expression<'_>,
    pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) {
    // Analyze the value being printed
    let value_pos = expression_analyzer::analyze(analyzer, expr, analysis_data, context);
    let value_type = analysis_data.expr_types.get(&value_pos).cloned();

    // `print` is a taint sink with the same kinds as `echo` (Psalm
    // PrintAnalyzer), wired Hakana-style through argument dataflow.
    if analyzer.config.taint_analysis
        && let Some(value_type) = value_type.as_ref()
    {
        output_constructs::add_output_call_argument_dataflow(
            analyzer,
            "print",
            0,
            value_pos,
            value_type,
            pos,
            analysis_data,
            context,
        );
    }

    // Psalm routes the printed value through ArgumentAnalyzer::verifyType
    // against a pseudo-param `string $var`.
    if let Some(t) = value_type.as_ref() {
        output_constructs::verify_output_argument_type(
            analyzer,
            t,
            value_pos,
            analysis_data,
            "print",
            0,
        );
    }

    // Psalm: a config listing `print` in forbiddenFunctions reports
    // ForbiddenCode for the construct.
    if output_constructs::is_forbidden_construct(analyzer, "print") {
        let (line, col) = analyzer.get_line_column(pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ForbiddenCode,
            "You have forbidden the use of print",
            analyzer.file_path,
            pos.0,
            pos.1,
            line,
            col,
        ));
    }

    // Psalm: `print` writes to output, so it is impure from a `@psalm-pure` context.
    output_constructs::emit_impure_output(analyzer, pos, analysis_data, "print");

    // print always returns 1
    analysis_data
        .expr_types
        .insert(pos, Rc::new(TUnion::new(TAtomic::TLiteralInt { value: 1 })));
}
