use pzoom_code_info::data_flow::node::FunctionLikeIdentifier;
use pzoom_code_info::{
    DataFlowGraph, DataFlowNode, DataFlowNodeId, DataFlowNodeKind, DataFlowNodePosition, GraphKind,
    PathKind, VarId, VariableSourceKind,
};

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

fn add_default_dataflow_path(
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

/// Build the function-body-graph source node Hakana's `functionlike_analyzer`
/// seeds for each parameter: a `Param`-id node with a `VariableUseSource`
/// kind. (Hakana sets `pure: false`, `has_parent_nodes: true` and
/// `has_awaitable: param_type.has_awaitable_types()`; PHP has no awaitables.)
fn make_param_source_node(
    kind: VariableSourceKind,
    name: VarId,
    pos: DataFlowNodePosition,
) -> DataFlowNode {
    DataFlowNode {
        id: DataFlowNodeId::Param(name, pos.file_path, pos.start_offset, pos.end_offset),
        kind: DataFlowNodeKind::VariableUseSource {
            pos,
            kind,
            pure: false,
            has_awaitable: false,
            has_await_call: false,
            has_parent_nodes: true,
            from_loop_init: false,
        },
    }
}

/// Seed a parameter's dataflow node and add it to the graph, returning the
/// node the param type should carry as parent. In the function-body graph
/// this is the `Param` `VariableUseSource` node above; in the whole-program
/// (taint) graph Hakana instead seeds a plain lvar vertex fed by the
/// function-like's argument node (`fn#N → $param`), so taints entering at a
/// call site reach the body's variable.
pub(crate) fn add_param_dataflow_node(
    graph: &mut DataFlowGraph,
    kind: VariableSourceKind,
    name: VarId,
    pos: DataFlowNodePosition,
    functionlike_id: Option<&FunctionLikeIdentifier>,
    param_index: usize,
    signature_type: Option<&pzoom_code_info::TUnion>,
) -> DataFlowNode {
    let parent_node = if let GraphKind::WholeProgram(_) = graph.kind {
        let parent_node = DataFlowNode::get_for_lvar(name, pos);

        if let Some(calling_id) = functionlike_id {
            let argument_node =
                DataFlowNode::get_for_method_argument(calling_id, param_index, Some(pos), None);
            // Psalm `FunctionLikeAnalyzer`: an int/float/bool-hinted param
            // strips all input taints except sleep on the `fn#N → $param`
            // edge (Union::getTaintsToRemove on the signature type).
            let removed_taints = signature_type
                .map(|t| t.get_taints_to_remove())
                .unwrap_or_default();
            graph.add_path(
                &argument_node.id,
                &parent_node.id,
                PathKind::Default,
                vec![],
                removed_taints,
            );
            graph.add_node(argument_node);
        }

        parent_node
    } else {
        make_param_source_node(kind, name, pos)
    };

    graph.add_node(parent_node.clone());
    parent_node
}

/// Psalm's mixed-issue origin (MixedIssueTrait::getMixedOriginMessage): when a
/// mixed value's dataflow traces back to exactly one origin node with a
/// position different from the issue's, the issue carries that origin as a
/// secondary location ("Consider improving the type here").
pub(crate) fn mixed_origin_secondary(
    analyzer: &crate::statements_analyzer::StatementsAnalyzer<'_>,
    analysis_data: &crate::function_analysis_data::FunctionAnalysisData,
    value_type: &pzoom_code_info::TUnion,
    issue_offset: u32,
) -> Option<pzoom_code_info::SecondaryLocation> {
    let mut origin_ids: Vec<pzoom_code_info::data_flow::node::DataFlowNodeId> = Vec::new();
    for parent_node in &value_type.parent_nodes {
        origin_ids.extend(analysis_data.data_flow_graph.get_origin_node_ids(
            &parent_node.id,
            &[],
            false,
        ));
    }
    origin_ids.sort();
    origin_ids.dedup();

    let mut origin_positions: Vec<DataFlowNodePosition> = origin_ids
        .iter()
        .filter_map(|id| {
            analysis_data
                .data_flow_graph
                .get_node(id)
                .and_then(|node| node.get_pos())
        })
        .collect();
    origin_positions.sort_by_key(|pos| (pos.file_path.0, pos.start_offset, pos.end_offset));
    origin_positions.dedup_by_key(|pos| (pos.file_path.0, pos.start_offset, pos.end_offset));

    // Psalm only reports a single unambiguous origin, and not when it is the
    // issue location itself.
    let [origin] = origin_positions.as_slice() else {
        return None;
    };
    if origin.file_path == analyzer.file_path && origin.start_offset == issue_offset {
        return None;
    }

    Some(pzoom_code_info::SecondaryLocation::new(
        pzoom_code_info::code_location::CodeLocation::new(
            origin.file_path,
            origin.start_offset,
            origin.end_offset,
            origin.start_line,
            origin.start_column as u32,
        ),
        "Consider improving the type here",
    ))
}
