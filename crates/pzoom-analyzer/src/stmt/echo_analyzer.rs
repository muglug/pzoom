//! Echo statement analyzer.

use mago_span::HasSpan;
use mago_syntax::ast::ast::echo::Echo;

use crate::context::BlockContext;
use crate::expr::echo_analyzer;
use crate::expression_analyzer;
use crate::function_analysis_data::FunctionAnalysisData;
use crate::statements_analyzer::{AnalysisError, StatementsAnalyzer};

/// Analyze an echo statement.
pub fn analyze(
    analyzer: &StatementsAnalyzer<'_>,
    echo: &Echo<'_>,
    analysis_data: &mut FunctionAnalysisData,
    context: &mut BlockContext,
) -> Result<(), AnalysisError> {
    // Mirrors Psalm `EchoAnalyzer`: echo writes to output and is impure in a
    // mutation-free context.
    let span = echo.span();
    echo_analyzer::emit_impure_output(
        analyzer,
        (span.start.offset, span.end.offset),
        analysis_data,
        "echo",
    );

    // Analyze each expression being echoed
    for (value_index, value) in echo.values.iter().enumerate() {
        // Hakana's echo_analyzer marks echoed expressions as general use.
        let was_inside_general_use = context.inside_general_use;
        context.inside_general_use = true;
        let pos = expression_analyzer::analyze(analyzer, value, analysis_data, context);
        context.inside_general_use = was_inside_general_use;
        if let Some(value_type) = analysis_data.expr_types.get(&pos).cloned() {
            echo_analyzer::check_stringable(analyzer, &value_type, pos, analysis_data, "echo");

            // Psalm routes echo arguments through verify_type, which reports
            // MixedArgument for mixed values (with the dataflow origin).
            // Gated on report_unused until pzoom's mixed-inference parity
            // catches up (see the foreach MixedAssignment gate).
            if analyzer.config.report_unused && value_type.is_mixed() {
                let (line, col) = analyzer.get_line_column(pos.0);
                let origin_secondary = crate::data_flow::mixed_origin_secondary(
                    analyzer,
                    analysis_data,
                    &value_type,
                    pos.0,
                );
                analysis_data.add_issue(
                    pzoom_code_info::Issue::new(
                        pzoom_code_info::IssueKind::MixedArgument,
                        "Argument 1 of echo cannot be mixed, expecting string",
                        analyzer.file_path,
                        pos.0,
                        pos.1,
                        line,
                        col,
                    )
                    .with_secondary_opt(origin_secondary),
                );
            }

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
                echo_analyzer::add_output_call_argument_dataflow(
                    analyzer,
                    "echo",
                    value_index,
                    pos,
                    &value_type,
                    (span.start.offset, span.end.offset),
                    analysis_data,
                    context,
                );
            }
        }
    }

    Ok(())
}
