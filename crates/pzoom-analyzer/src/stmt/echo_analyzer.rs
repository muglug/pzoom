//! Echo statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::echo::{Echo, EchoTag};
use mago_syntax::ast::ast::expression::Expression;

use pzoom_code_info::{Issue, IssueKind};

use crate::context::BlockContext;
use crate::expr::output_constructs;
use crate::expression_analyzer;
use crate::function_analysis_data::{FunctionAnalysisData, Pos};
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an echo statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    echo: &Echo<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let span = echo.span();
    analyze_values(
        analyzer,
        echo.values.iter(),
        (span.start.offset, span.end.offset),
        analysis_data,
        context,
    )
}

/// Analyze a `<?= ... ?>` echo tag; php-parser gives Psalm the same Echo_
/// statement for both spellings.
pub fn analyze_tag(
    analyzer: &StatementsAnalyzer<'_>,
    tag: &EchoTag<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    let span = tag.span();
    analyze_values(
        analyzer,
        tag.values.iter(),
        (span.start.offset, span.end.offset),
        analysis_data,
        context,
    )
}

fn analyze_values<'ast, 'arena>(
    analyzer: &StatementsAnalyzer<'_>,
    values: impl Iterator<Item = &'ast Expression<'arena>>,
    stmt_pos: Pos,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError>
where
    'arena: 'ast,
{
    // Analyze each expression being echoed
    for (value_index, value) in values.enumerate() {
        // Hakana's echo_analyzer marks echoed expressions as general use.
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;
        let pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        context.inside_general_use = was_inside_general_use;
        if let Some(value_type) = analysis_data.expr_types.get(&pos).cloned() {
            // Psalm routes echo arguments through ArgumentAnalyzer::verifyType
            // against a pseudo-param `string $var`, reporting MixedArgument
            // for mixed values (with the dataflow origin) and
            // (Possibly)InvalidArgument for non-stringable ones.
            output_constructs::verify_output_argument_type(
                analyzer,
                &value_type,
                pos,
                analysis_data,
                "echo",
                value_index,
            );

            // The echoed value is consumed (Hakana routes echo args through
            // argument_analyzer::verify_type, which adds a variable-use sink).
            if analysis_data.data_flow_graph.kind == pzoom_code_info::GraphKind::FunctionBody
                && !value_type.parent_nodes.is_empty()
            {
                let echo_sink = pzoom_code_info::DataFlowNode::get_for_unlabelled_sink(
                    crate::data_flow::make_data_flow_node_position(analyzer, pos),
                );
                for parent_node in &value_type.parent_nodes {
                    analysis_data.data_flow_graph.add_path(
                        &parent_node.id,
                        &echo_sink.id,
                        pzoom_code_info::PathKind::Default,
                        vec![],
                        vec![],
                    );
                }
                analysis_data.data_flow_graph.add_node(echo_sink);
            }

            // Hakana routes echo arguments through
            // `argument_analyzer::add_dataflow` with a pseudo-`echo`
            // function-like whose param is a taint sink; the sink kinds are
            // Psalm's (`EchoAnalyzer`: html, has_quotes, user_secret,
            // system_secret), since the corpus is Psalm's TaintTest.
            if analyzer.config.taint_analysis {
                output_constructs::add_output_call_argument_dataflow(
                    analyzer,
                    "echo",
                    value_index,
                    pos,
                    &value_type,
                    stmt_pos,
                    analysis_data,
                    context,
                );
            }
        }
    }

    // Psalm: a config listing `echo` in forbiddenFunctions reports
    // ForbiddenCode for the statement.
    if output_constructs::is_forbidden_construct(analyzer, "echo") {
        let (line, col) = analyzer.get_line_column(stmt_pos.0);
        analysis_data.add_issue(Issue::new(
            IssueKind::ForbiddenCode,
            "Use of echo",
            analyzer.file_path,
            stmt_pos.0,
            stmt_pos.1,
            line,
            col,
        ));
    }

    // Mirrors Psalm `EchoAnalyzer`: echo writes to output and is impure in a
    // mutation-free context.
    output_constructs::emit_impure_output(analyzer, stmt_pos, analysis_data, "echo");

    Ok(())
}
