//! Type operations module - combining, comparing, and manipulating types.

mod type_combination;
pub mod type_combiner;

use crate::data_flow::node::DataFlowNode;

pub use type_combiner::{add_union_type, combine, combine_union_types};

pub fn extend_dataflow_uniquely(
    type_1_nodes: &mut Vec<DataFlowNode>,
    type_2_nodes: Vec<DataFlowNode>,
) {
    type_1_nodes.extend(type_2_nodes);
    type_1_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    type_1_nodes.dedup_by(|a, b| a.id.eq(&b.id));
}
