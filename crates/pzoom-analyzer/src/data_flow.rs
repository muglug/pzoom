use pzoom_code_info::{DataFlowGraph, DataFlowNode, DataFlowNodePosition, PathKind};

use crate::{function_analysis_data::Pos, statements_analyzer::StatementsAnalyzer};

pub(crate) fn make_data_flow_node_position(
    analyzer: &StatementsAnalyzer<'_>,
    pos: Pos,
) -> DataFlowNodePosition {
    let (start_line, start_column) = analyzer.get_line_column(pos.0);
    let (end_line, end_column) = analyzer.get_line_column(pos.1);

    DataFlowNodePosition::new(
        analyzer.file_path,
        pos.0,
        pos.1,
        start_line,
        end_line,
        start_column as u16,
        end_column as u16,
    )
}

pub(crate) fn add_default_dataflow_path(
    data_flow_graph: &mut DataFlowGraph,
    from: &DataFlowNode,
    to: &DataFlowNode,
) {
    data_flow_graph.add_path(&from.id, &to.id, PathKind::Default, vec![], vec![]);
}

pub(crate) fn add_default_dataflow_paths(
    data_flow_graph: &mut DataFlowGraph,
    from_nodes: &[DataFlowNode],
    to: &DataFlowNode,
) {
    for from in from_nodes {
        add_default_dataflow_path(data_flow_graph, from, to);
    }
}
