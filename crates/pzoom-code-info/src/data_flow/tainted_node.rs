use std::{collections::BTreeSet, rc::Rc};

use pzoom_str::Interner;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    node::{DataFlowNode, DataFlowNodeId, DataFlowNodeKind, SinkType, SourceType, lookup_id},
    path::PathKind,
};

use pzoom_str::StrId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaintedNode {
    pub id: DataFlowNodeId,
    pub pos: Option<Rc<super::node::DataFlowNodePosition>>,
    pub is_specialized: bool,
    pub taint_sources: Vec<SourceType>,
    pub taint_sinks: Vec<SinkType>,
    pub previous: Option<Rc<TaintedNode>>,
    pub path_types: Vec<PathKind>,
    pub specialized_calls: FxHashMap<(StrId, u32), FxHashSet<DataFlowNodeId>>,
}

impl TaintedNode {
    /// Port of Psalm `TaintFlowGraph::getPredecessorPath`: `label (file:line:col)`
    /// segments joined with ` -> `. Consecutive nodes at the same location
    /// collapse (the earlier one is skipped) when the earlier node has its own
    /// predecessor.
    pub fn get_trace(&self, interner: &Interner) -> String {
        let source_descriptor = format!(
            "{}{}",
            self.id.to_label(interner),
            if let Some(pos) = &self.pos {
                format!(
                    " ({}:{}:{})",
                    lookup_id(interner, pos.file_path),
                    pos.start_line,
                    pos.start_column
                )
            } else {
                "".to_string()
            }
        );

        if let Some(previous_source) = &self.previous {
            if let (Some(pos), Some(prev_pos)) = (&self.pos, &previous_source.pos)
                && pos.file_path == prev_pos.file_path
                && pos.start_offset == prev_pos.start_offset
                && pos.end_offset == prev_pos.end_offset
                && let Some(prev_prev) = &previous_source.previous
            {
                return format!("{} -> {}", prev_prev.get_trace(interner), source_descriptor);
            }

            return format!(
                "{} -> {}",
                previous_source.get_trace(interner),
                source_descriptor
            );
        }

        source_descriptor
    }

    /// The trace as structured nodes in source-to-sink order, with the same
    /// same-location collapsing as [`Self::get_trace`]. Feeds the console
    /// reporter's Psalm-style taint snippets.
    pub fn get_trace_nodes(
        &self,
        interner: &Interner,
    ) -> Vec<(String, Option<super::node::DataFlowNodePosition>)> {
        let mut nodes = Vec::new();
        self.collect_trace_nodes(interner, &mut nodes);
        nodes.reverse();
        nodes
    }

    fn collect_trace_nodes(
        &self,
        interner: &Interner,
        nodes: &mut Vec<(String, Option<super::node::DataFlowNodePosition>)>,
    ) {
        nodes.push((self.id.to_label(interner), self.pos.as_deref().copied()));

        if let Some(previous_source) = &self.previous {
            if let (Some(pos), Some(prev_pos)) = (&self.pos, &previous_source.pos)
                && pos.file_path == prev_pos.file_path
                && pos.start_offset == prev_pos.start_offset
                && pos.end_offset == prev_pos.end_offset
                && let Some(prev_prev) = &previous_source.previous
            {
                prev_prev.collect_trace_nodes(interner, nodes);
                return;
            }

            previous_source.collect_trace_nodes(interner, nodes);
        }
    }

    pub fn get_taint_sources(&self) -> &Vec<SourceType> {
        if let Some(previous_source) = &self.previous {
            return previous_source.get_taint_sources();
        }

        &self.taint_sources
    }

    pub fn from(node: &DataFlowNode) -> Self {
        match &node.kind {
            DataFlowNodeKind::Vertex {
                pos,
                is_specialized,
            } => TaintedNode {
                id: node.id.clone(),
                pos: pos.as_ref().map(|p| Rc::new(*p)),
                is_specialized: *is_specialized,
                taint_sinks: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::TaintSource { pos, types, .. } => TaintedNode {
                id: node.id.clone(),
                pos: pos.as_ref().map(|p| Rc::new(*p)),
                is_specialized: false,
                // Psalm's source taints ARE the sink kinds they can reach.
                taint_sinks: types.clone(),
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::TaintSink { pos, types, .. } => TaintedNode {
                id: node.id.clone(),
                pos: pos.as_ref().map(|p| Rc::new(*p)),
                is_specialized: false,
                taint_sinks: types.clone(),
                taint_sources: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
            },
            DataFlowNodeKind::DataSource { pos, target_id, .. } => TaintedNode {
                id: node.id.clone(),
                pos: Some(Rc::new(*pos)),
                is_specialized: false,
                taint_sinks: vec![SinkType::Custom(target_id.clone())],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::VariableUseSource { pos, .. } => TaintedNode {
                id: node.id.clone(),
                pos: Some(Rc::new(*pos)),
                is_specialized: false,
                taint_sinks: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::VariableUseSink { pos } => TaintedNode {
                id: node.id.clone(),
                pos: Some(Rc::new(*pos)),
                is_specialized: false,
                taint_sinks: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
            DataFlowNodeKind::ForLoopInit { .. } => TaintedNode {
                id: node.id.clone(),
                pos: None,
                is_specialized: false,
                taint_sinks: vec![],
                previous: None,
                path_types: Vec::new(),
                specialized_calls: FxHashMap::default(),
                taint_sources: vec![],
            },
        }
    }

    pub fn get_unique_source_id(&self, interner: &Interner) -> String {
        let mut id = self.id.to_string(interner)
            + "|"
            + self
                .path_types
                .iter()
                .filter(|t| !matches!(t, PathKind::Default))
                .map(|k| k.to_unique_string())
                .collect::<Vec<_>>()
                .join("-")
                .as_str()
            + "|";

        for taint_type in self
            .taint_sinks
            .iter()
            .map(|t| format!("{:?}", t))
            .collect::<BTreeSet<_>>()
        {
            id += "-";
            id += taint_type.as_str();
        }

        id += "|";

        for specialization in self
            .specialized_calls
            .iter()
            .map(|t| format!("{}:{}", t.0.0.0, t.0.1))
            .collect::<BTreeSet<_>>()
        {
            id += "-";
            id += &specialization;
        }

        id
    }
}
