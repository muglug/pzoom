//! Unused-variable detection over the function-body data flow graph.
//!
//! Port of Hakana's `dataflow/unused_variable_analyzer.rs` (`check_variables_used`
//! / `is_variable_used`) with the path-ignore helpers from its
//! `dataflow/program_analyzer.rs`. Assignments and parameters appear in the
//! graph as `VariableUseSource` nodes; consuming contexts (call arguments,
//! conditions, returns, general-use variable fetches, …) appear as sinks. A
//! source whose forward closure never reaches a sink is unused. Psalm reaches
//! the same verdicts through its `VariableUseGraph::isVariableUsed`; Hakana's
//! walk additionally distinguishes never-referenced sources from
//! referenced-but-not-used ones and filters array/property fetch paths that
//! don't correspond to an earlier assignment.

use pzoom_code_info::data_flow::graph::DataFlowGraph;
use pzoom_code_info::data_flow::node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind};
use pzoom_code_info::data_flow::path::ArrayDataKind;
use pzoom_code_info::PathKind;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;

enum VariableUsage {
    NeverReferenced,
    ReferencedButNotUsed,
    Used,
}

/// Walks every `VariableUseSource` in the graph. Returns
/// `(unused, unused_but_referenced)` source nodes, mirroring Hakana's
/// `check_variables_used`: a never-referenced pure default-kind source counts
/// as plainly unused; everything else that fails to reach a sink lands in the
/// referenced-but-not-used bucket.
pub(crate) fn check_variables_used(graph: &DataFlowGraph) -> (Vec<DataFlowNode>, Vec<DataFlowNode>) {
    if std::env::var("PZOOM_DEBUG_USE_GRAPH").is_ok() {
        eprintln!("=== use graph ===");
        for (from, edges) in &graph.forward_edges {
            for (to, path) in edges {
                eprintln!("  {:?} -> {:?} [{:?}]", from, to, path.kind);
            }
        }
        eprintln!("  sources: {:?}", graph.sources.keys().collect::<Vec<_>>());
        eprintln!("  sinks: {:?}", graph.sinks.keys().collect::<Vec<_>>());
    }
    let vars = graph
        .sources
        .iter()
        .filter_map(|(_, source)| match &source.kind {
            DataFlowNodeKind::VariableUseSource { pos, .. } => {
                Some(((pos.start_offset, pos.end_offset), source))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    let mut unused_nodes = Vec::new();
    let mut unused_but_referenced_nodes = Vec::new();

    for (_, source_node) in vars {
        match is_variable_used(graph, source_node) {
            VariableUsage::NeverReferenced => {
                if let DataFlowNodeKind::VariableUseSource {
                    pure: true,
                    kind: pzoom_code_info::VariableSourceKind::Default,
                    ..
                } = source_node.kind
                {
                    unused_nodes.push(source_node.clone());
                } else {
                    unused_but_referenced_nodes.push(source_node.clone());
                }
            }
            VariableUsage::ReferencedButNotUsed => {
                unused_but_referenced_nodes.push(source_node.clone());
            }
            VariableUsage::Used => {}
        }
    }

    (unused_nodes, unused_but_referenced_nodes)
}

/// The per-walk node state: the path kinds traversed from the source so far
/// (Hakana's `VariableUseNode`).
#[derive(Clone)]
struct VariableUseNode {
    path_types: Vec<PathKind>,
}

fn is_variable_used(graph: &DataFlowGraph, source_node: &DataFlowNode) -> VariableUsage {
    let mut visited_source_ids: FxHashSet<DataFlowNodeId> = FxHashSet::default();

    let mut sources: FxHashMap<DataFlowNodeId, VariableUseNode> = FxHashMap::default();
    sources.insert(
        source_node.id.clone(),
        VariableUseNode {
            path_types: Vec::new(),
        },
    );

    let mut i = 0;

    while i < 200 {
        if sources.is_empty() {
            break;
        }

        let mut new_child_nodes = FxHashMap::default();

        for (id, source) in &sources {
            visited_source_ids.insert(id.clone());

            let child_nodes = get_variable_child_nodes(graph, id, source, &visited_source_ids);

            if let Some(child_nodes) = child_nodes {
                new_child_nodes.extend(child_nodes);
            } else {
                return VariableUsage::Used;
            }
        }

        sources = new_child_nodes;

        i += 1;
    }

    if i == 1 {
        VariableUsage::NeverReferenced
    } else {
        VariableUsage::ReferencedButNotUsed
    }
}

/// `None` means a sink was reached (the variable is used); otherwise the next
/// frontier of nodes to walk.
fn get_variable_child_nodes(
    graph: &DataFlowGraph,
    generated_source_id: &DataFlowNodeId,
    generated_source: &VariableUseNode,
    visited_source_ids: &FxHashSet<DataFlowNodeId>,
) -> Option<FxHashMap<DataFlowNodeId, VariableUseNode>> {
    let mut new_child_nodes = FxHashMap::default();

    if let Some(forward_edges) = graph.forward_edges.get(generated_source_id) {
        for (to_id, path) in forward_edges {
            if graph.sinks.contains_key(to_id) {
                return None;
            }

            if visited_source_ids.contains(to_id) {
                continue;
            }

            if should_ignore_array_fetch(
                &path.kind,
                &ArrayDataKind::ArrayKey,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_array_fetch(
                &path.kind,
                &ArrayDataKind::ArrayValue,
                &generated_source.path_types,
            ) {
                continue;
            }

            if should_ignore_property_fetch(&path.kind, &generated_source.path_types) {
                continue;
            }

            let mut new_destination = VariableUseNode {
                path_types: generated_source.path_types.clone(),
            };
            new_destination.path_types.push(path.kind.clone());

            new_child_nodes.insert(to_id.clone(), new_destination);
        }
    }

    Some(new_child_nodes)
}

/// An array-key/value fetch path only carries data if a matching assignment
/// happened earlier at the same nesting level (Hakana
/// `program_analyzer::should_ignore_array_fetch`).
pub(crate) fn should_ignore_array_fetch(
    path_type: &PathKind,
    match_type: &ArrayDataKind,
    previous_path_types: &[PathKind],
) -> bool {
    if match path_type {
        PathKind::ArrayFetch(inner_expression_type, _) => inner_expression_type == match_type,
        PathKind::UnknownArrayFetch(ArrayDataKind::ArrayKey) => {
            match_type == &ArrayDataKind::ArrayValue
        }
        _ => false,
    } {
        let mut fetch_nesting = 0;

        for previous_path_type in previous_path_types.iter().rev() {
            match previous_path_type {
                PathKind::UnknownArrayAssignment(inner) => {
                    if inner == match_type {
                        if fetch_nesting == 0 {
                            return false;
                        }

                        fetch_nesting -= 1;
                    }
                }
                PathKind::ArrayAssignment(inner, previous_assignment_value) => {
                    if inner == match_type {
                        if fetch_nesting > 0 {
                            fetch_nesting -= 1;
                            continue;
                        }

                        if let PathKind::ArrayFetch(_, fetch_value) = path_type {
                            if fetch_value == previous_assignment_value {
                                return false;
                            }
                        }

                        return true;
                    }
                }
                PathKind::UnknownArrayFetch(inner) | PathKind::ArrayFetch(inner, _) => {
                    if inner == match_type {
                        fetch_nesting += 1;
                    }
                }
                _ => {}
            }
        }
    }

    if let PathKind::RemoveDictKey(key_name) = path_type
        && match_type == &ArrayDataKind::ArrayValue
        && let Some(PathKind::ArrayAssignment(ArrayDataKind::ArrayValue, assigned_name)) =
            previous_path_types
                .iter()
                .rfind(|t| !matches!(t, PathKind::Default))
        && assigned_name == key_name
    {
        return true;
    }

    false
}

/// A property fetch path only carries data if a matching property assignment
/// happened earlier (Hakana `program_analyzer::should_ignore_property_fetch`).
pub(crate) fn should_ignore_property_fetch(
    path_type: &PathKind,
    previous_path_types: &[PathKind],
) -> bool {
    if let PathKind::PropertyFetch(_, fetch_value) = path_type {
        let mut fetch_nesting = 0;

        for previous_path_type in previous_path_types.iter().rev() {
            match previous_path_type {
                PathKind::UnknownPropertyAssignment => {
                    if fetch_nesting == 0 {
                        return false;
                    }

                    fetch_nesting -= 1;
                }
                PathKind::PropertyAssignment(_, previous_assignment_value) => {
                    if fetch_nesting > 0 {
                        fetch_nesting -= 1;
                        continue;
                    }

                    if fetch_value == previous_assignment_value {
                        return false;
                    }

                    return true;
                }
                PathKind::UnknownPropertyFetch | PathKind::PropertyFetch(_, _) => {
                    fetch_nesting += 1;
                }
                _ => {}
            }
        }
    }

    false
}
