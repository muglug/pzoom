//! Generic recursion over a type tree, modelled on Hakana's `TypeNode` /
//! `get_all_child_nodes` (itself a Rust port of Psalm's `Type\TypeNode` and
//! `Internal\TypeVisitor`).
//!
//! A [`TypeNode`] is either a whole union or one of its atomic members.
//! [`TypeNode::get_child_nodes`] yields the immediate children one level down
//! the tree (an array's key/value unions, a generic object's params, a
//! callable's params/return, a template's bound, …), and [`visit_type_tree`]
//! walks the whole tree in pre-order. Analyses that must inspect every class
//! named anywhere inside a type (deprecation, undefined/wrong-cased classes)
//! drive this rather than re-implementing the descent each time.

use crate::t_atomic::TAtomic;
use crate::t_union::TUnion;

/// A node in a type tree: a whole union, or a single atomic within one.
#[derive(Clone, Copy)]
pub enum TypeNode<'a> {
    Union(&'a TUnion),
    Atomic(&'a TAtomic),
}

impl<'a> TypeNode<'a> {
    /// The immediate child nodes one level down (Hakana's `get_all_child_nodes`).
    pub fn get_child_nodes(&self) -> Vec<TypeNode<'a>> {
        match self {
            TypeNode::Union(union) => union.get_child_nodes(),
            TypeNode::Atomic(atomic) => atomic.get_child_nodes(),
        }
    }
}

impl TUnion {
    /// The atomics of this union, as child type nodes (Hakana's
    /// `TUnion::get_all_child_nodes`).
    pub fn get_child_nodes(&self) -> Vec<TypeNode<'_>> {
        self.types.iter().map(TypeNode::Atomic).collect()
    }
}

impl TAtomic {
    /// The type nodes nested directly inside this atomic — generic params,
    /// array element/key types, shape fields, callable params/returns, template
    /// bounds, class-string and conditional subtypes. Mirrors Hakana's
    /// `TAtomic::get_all_child_nodes` (Psalm's `Type\Atomic::getChildNodes`).
    pub fn get_child_nodes(&self) -> Vec<TypeNode<'_>> {
        match self {
            TAtomic::TIterable {
                key_type,
                value_type,
            } => vec![TypeNode::Union(key_type), TypeNode::Union(value_type)],
            TAtomic::TArray {
                known_values,
                params,
                ..
            } => {
                let mut nodes: Vec<TypeNode<'_>> = known_values
                    .values()
                    .map(|(_, value)| TypeNode::Union(value))
                    .collect();
                if let Some(params) = params {
                    nodes.push(TypeNode::Union(&params.0));
                    nodes.push(TypeNode::Union(&params.1));
                }
                nodes
            }
            TAtomic::TObjectWithProperties { properties, .. } => {
                properties.values().map(TypeNode::Union).collect()
            }
            TAtomic::TClassStringMap {
                as_type,
                value_param,
                ..
            } => {
                let mut nodes = vec![TypeNode::Union(value_param)];
                if let Some(as_type) = as_type {
                    nodes.push(TypeNode::Atomic(as_type));
                }
                nodes
            }
            TAtomic::TNamedObject {
                type_params: Some(type_params),
                ..
            } => type_params.iter().map(TypeNode::Union).collect(),
            TAtomic::TObjectIntersection { types } => types.iter().map(TypeNode::Atomic).collect(),
            TAtomic::TClassString {
                as_type: Some(as_type),
            }
            | TAtomic::TTemplateParamClass { as_type, .. } => vec![TypeNode::Atomic(as_type)],
            TAtomic::TTemplateParam { as_type, .. }
            | TAtomic::TTemplateKeyOf { as_type, .. }
            | TAtomic::TTemplateValueOf { as_type, .. }
            | TAtomic::TDependentGetClass { as_type, .. } => vec![TypeNode::Union(as_type)],
            TAtomic::TCallable {
                params,
                return_type,
                ..
            }
            | TAtomic::TClosure {
                params,
                return_type,
                ..
            } => {
                let mut nodes = Vec::new();
                if let Some(params) = params {
                    nodes.extend(
                        params
                            .iter()
                            .map(|param| TypeNode::Union(&param.param_type)),
                    );
                }
                if let Some(return_type) = return_type {
                    nodes.push(TypeNode::Union(return_type));
                }
                nodes
            }
            TAtomic::TConditional(conditional) => vec![
                TypeNode::Union(&conditional.as_type),
                TypeNode::Union(&conditional.conditional_type),
                TypeNode::Union(&conditional.if_true_type),
                TypeNode::Union(&conditional.if_false_type),
            ],
            _ => Vec::new(),
        }
    }
}

/// Pre-order walk of every node in a type tree, mirroring Psalm's
/// `TypeVisitor::traverse` (ported via Hakana). `visit` is called on each node
/// and returns whether to descend into that node's children, letting a visitor
/// prune subtrees (Psalm's `DONT_TRAVERSE_CHILDREN`).
pub fn visit_type_tree<F>(node: &TypeNode<'_>, visit: &mut F)
where
    F: FnMut(&TypeNode<'_>) -> bool,
{
    if visit(node) {
        for child in node.get_child_nodes() {
            visit_type_tree(&child, visit);
        }
    }
}
