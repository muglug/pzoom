//! Type operations module - combining, comparing, and manipulating types.

pub mod key_value_of;
pub mod template;
mod type_combination;
pub mod type_combiner;
pub mod type_node;

use crate::data_flow::node::DataFlowNode;

pub use key_value_of::{get_key_of_union, get_value_of_union};
pub use type_node::{visit_type_tree, TypeNode};
pub use type_combiner::{
    add_union_type, combine, combine_union_types, combine_union_types_with_codebase,
    combine_with_codebase,
};

pub fn extend_dataflow_uniquely(
    type_1_nodes: &mut Vec<DataFlowNode>,
    type_2_nodes: Vec<DataFlowNode>,
) {
    type_1_nodes.extend(type_2_nodes);
    type_1_nodes.sort_by(|a, b| a.id.cmp(&b.id));
    type_1_nodes.dedup_by(|a, b| a.id.eq(&b.id));
}
