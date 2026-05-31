//! Parse-tree data model.
//!
//! Faithful port of Psalm's `Internal/Type/ParseTree` class hierarchy. Psalm's
//! `ParseTreeCreator` mutates a tree of nodes through `parent` pointers
//! (`$node->parent`, `$parent->children[] = ...`, `array_pop($parent->children)`,
//! reassigning `->parent`). Rust can't express that aliasing ergonomically with
//! `Box`/`Rc`, so we store every node in an arena ([`ParseTreeArena`]) and refer
//! to nodes by [`NodeId`]. Each arena operation maps one-to-one onto a Psalm
//! pointer operation, which lets [`super::parse_tree_creator`] follow Psalm
//! block-by-block.

/// Index of a node within a [`ParseTreeArena`]. Stands in for a `ParseTree`
/// object reference in Psalm.
pub type NodeId = usize;

/// The node variants, one per Psalm `ParseTree` subclass.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// `ParseTree\Root`
    Root,
    /// `ParseTree\Value`
    Value {
        value: String,
        offset_start: usize,
        offset_end: usize,
        /// Pre-resolution original text (`array{..., 2: string}` token element).
        text: Option<String>,
    },
    /// `ParseTree\GenericTree`
    Generic { value: String },
    /// `ParseTree\UnionTree`
    Union,
    /// `ParseTree\IntersectionTree`
    Intersection,
    /// `ParseTree\NullableTree`
    Nullable,
    /// `ParseTree\KeyedArrayTree`
    KeyedArray { value: String },
    /// `ParseTree\KeyedArrayPropertyTree`
    KeyedArrayProperty { value: String },
    /// `ParseTree\CallableTree`
    Callable { value: String },
    /// `ParseTree\CallableParamTree`
    CallableParam {
        variadic: bool,
        has_default: bool,
        /// Param name without the `$` prefix.
        name: Option<String>,
    },
    /// `ParseTree\CallableWithReturnTypeTree`
    CallableWithReturnType,
    /// `ParseTree\EncapsulationTree`
    Encapsulation,
    /// `ParseTree\MethodTree`
    Method { value: String },
    /// `ParseTree\MethodParamTree`
    MethodParam {
        name: String,
        byref: bool,
        variadic: bool,
        default: String,
    },
    /// `ParseTree\MethodWithReturnTypeTree`
    MethodWithReturnType,
    /// `ParseTree\TemplateAsTree`
    TemplateAs { param_name: String, as_type: String },
    /// `ParseTree\TemplateIsTree`
    TemplateIs { param_name: String },
    /// `ParseTree\ConditionalTree` (its `condition` is a `TemplateIs` node).
    Conditional { condition: NodeId },
    /// `ParseTree\IndexedAccessTree`
    IndexedAccess { value: String },
    /// `ParseTree\FieldEllipsis`
    FieldEllipsis,
}

/// A single tree node. `parent`/`children` mirror Psalm's `ParseTree` base
/// fields; `terminated` and `possibly_undefined` mirror the booleans declared
/// on the relevant subclasses (`terminated` only matters for
/// Generic/KeyedArray/Callable/Encapsulation; `possibly_undefined` only for
/// KeyedArrayProperty).
#[derive(Debug, Clone)]
pub struct Node {
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub terminated: bool,
    pub possibly_undefined: bool,
}

impl Node {
    fn new(kind: NodeKind, parent: Option<NodeId>) -> Self {
        Self {
            kind,
            parent,
            children: Vec::new(),
            terminated: false,
            possibly_undefined: false,
        }
    }
}

/// The arena holding every node of a parse tree.
#[derive(Debug, Clone, Default)]
pub struct ParseTreeArena {
    nodes: Vec<Node>,
}

impl ParseTreeArena {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Allocate a node (Psalm `new SomeTree($parent)`). Like Psalm, the new node
    /// is *not* automatically appended to the parent's `children`; the caller
    /// does that explicitly, mirroring `$parent->children[] = $new_leaf`.
    pub fn alloc(&mut self, kind: NodeKind, parent: Option<NodeId>) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(Node::new(kind, parent));
        id
    }

    #[inline]
    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }

    #[inline]
    pub fn kind(&self, id: NodeId) -> &NodeKind {
        &self.nodes[id].kind
    }

    #[inline]
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].parent
    }

    #[inline]
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) {
        self.nodes[id].parent = parent;
    }

    #[inline]
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.nodes[id].children
    }

    #[inline]
    pub fn push_child(&mut self, id: NodeId, child: NodeId) {
        self.nodes[id].children.push(child);
    }

    /// `array_pop($node->children)`.
    #[inline]
    pub fn pop_child(&mut self, id: NodeId) -> Option<NodeId> {
        self.nodes[id].children.pop()
    }

    /// Replace `node`'s children wholesale (`$node->children = [...]`).
    #[inline]
    pub fn set_children(&mut self, id: NodeId, children: Vec<NodeId>) {
        self.nodes[id].children = children;
    }

    #[inline]
    pub fn terminated(&self, id: NodeId) -> bool {
        self.nodes[id].terminated
    }

    #[inline]
    pub fn set_terminated(&mut self, id: NodeId, value: bool) {
        self.nodes[id].terminated = value;
    }

    #[inline]
    pub fn possibly_undefined(&self, id: NodeId) -> bool {
        self.nodes[id].possibly_undefined
    }

    #[inline]
    pub fn set_possibly_undefined(&mut self, id: NodeId, value: bool) {
        self.nodes[id].possibly_undefined = value;
    }

    // ---- `instanceof` helpers --------------------------------------------

    pub fn is_root(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Root)
    }
    pub fn is_value(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Value { .. })
    }
    pub fn is_generic(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Generic { .. })
    }
    pub fn is_union(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Union)
    }
    pub fn is_intersection(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Intersection)
    }
    pub fn is_nullable(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Nullable)
    }
    pub fn is_keyed_array(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::KeyedArray { .. })
    }
    pub fn is_keyed_array_property(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::KeyedArrayProperty { .. })
    }
    pub fn is_callable(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Callable { .. })
    }
    pub fn is_callable_param(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::CallableParam { .. })
    }
    pub fn is_callable_with_return_type(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::CallableWithReturnType)
    }
    pub fn is_encapsulation(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Encapsulation)
    }
    pub fn is_method(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Method { .. })
    }
    pub fn is_method_with_return_type(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::MethodWithReturnType)
    }
    pub fn is_template_is(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::TemplateIs { .. })
    }
    pub fn is_conditional(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::Conditional { .. })
    }
    pub fn is_field_ellipsis(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, NodeKind::FieldEllipsis)
    }

    /// `$node->value` for the node kinds that carry one.
    pub fn value(&self, id: NodeId) -> Option<&str> {
        match &self.nodes[id].kind {
            NodeKind::Value { value, .. }
            | NodeKind::Generic { value }
            | NodeKind::KeyedArray { value }
            | NodeKind::KeyedArrayProperty { value }
            | NodeKind::Callable { value }
            | NodeKind::Method { value }
            | NodeKind::IndexedAccess { value } => Some(value),
            _ => None,
        }
    }
}
