//! Parse tree types for type parsing.
//!
//! Based on Psalm's ParseTree types.

/// A node in the type parse tree.
#[derive(Debug, Clone)]
pub enum ParseTree {
    /// Root node
    Root(ParseTreeNode),
    /// A simple value (type name, literal, etc.)
    Value(ValueNode),
    /// A generic type like `array<K, V>`
    Generic(GenericNode),
    /// A union type like `A|B`
    Union(ParseTreeNode),
    /// An intersection type like `A&B`
    Intersection(ParseTreeNode),
    /// A nullable type like `?T`
    Nullable(ParseTreeNode),
    /// A keyed array (shape) like `array{foo: T}`
    KeyedArray(KeyedArrayNode),
    /// A property in a keyed array
    KeyedArrayProperty(KeyedArrayPropertyNode),
    /// A callable type like `callable(T): R`
    Callable(CallableNode),
    /// A callable with return type
    CallableWithReturnType(ParseTreeNode),
    /// A callable parameter
    CallableParam(CallableParamNode),
    /// Parenthesized type
    Encapsulation(EncapsulationNode),
    /// A method type (for @method)
    Method(MethodNode),
    /// A method with return type
    MethodWithReturnType(ParseTreeNode),
    /// A method parameter
    MethodParam(MethodParamNode),
    /// Template "as" clause like `T as SomeClass`
    TemplateAs(TemplateAsNode),
    /// Template "is" clause for conditionals
    TemplateIs(TemplateIsNode),
    /// Conditional type like `T is string ? A : B`
    Conditional(ConditionalNode),
    /// Indexed access like `T[K]`
    IndexedAccess(IndexedAccessNode),
    /// Field ellipsis in keyed array `...`
    FieldEllipsis(ParseTreeNode),
}

/// Common node data
#[derive(Debug, Clone, Default)]
pub struct ParseTreeNode {
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl ParseTreeNode {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            terminated: false,
        }
    }
}

/// A value node (type name, literal)
#[derive(Debug, Clone)]
pub struct ValueNode {
    pub value: String,
    pub offset_start: usize,
    pub offset_end: usize,
    pub original_text: Option<String>,
}

impl ValueNode {
    pub fn new(value: impl Into<String>, offset_start: usize, offset_end: usize) -> Self {
        Self {
            value: value.into(),
            offset_start,
            offset_end,
            original_text: None,
        }
    }

    pub fn with_text(
        value: impl Into<String>,
        offset_start: usize,
        offset_end: usize,
        text: Option<String>,
    ) -> Self {
        Self {
            value: value.into(),
            offset_start,
            offset_end,
            original_text: text,
        }
    }
}

/// A generic type node
#[derive(Debug, Clone)]
pub struct GenericNode {
    pub value: String,
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl GenericNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
            terminated: false,
        }
    }
}

/// A keyed array (shape) node
#[derive(Debug, Clone)]
pub struct KeyedArrayNode {
    pub value: String,
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl KeyedArrayNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
            terminated: false,
        }
    }
}

/// A property in a keyed array
#[derive(Debug, Clone)]
pub struct KeyedArrayPropertyNode {
    pub value: String,
    pub children: Vec<ParseTree>,
    pub possibly_undefined: bool,
}

impl KeyedArrayPropertyNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
            possibly_undefined: false,
        }
    }
}

/// A callable type node
#[derive(Debug, Clone)]
pub struct CallableNode {
    pub value: String,
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl CallableNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
            terminated: false,
        }
    }
}

/// A callable parameter node
#[derive(Debug, Clone)]
pub struct CallableParamNode {
    pub children: Vec<ParseTree>,
    pub variadic: bool,
    pub has_default: bool,
    pub name: Option<String>,
}

impl CallableParamNode {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            variadic: false,
            has_default: false,
            name: None,
        }
    }
}

impl Default for CallableParamNode {
    fn default() -> Self {
        Self::new()
    }
}

/// Encapsulation (parenthesized type)
#[derive(Debug, Clone)]
pub struct EncapsulationNode {
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl EncapsulationNode {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            terminated: false,
        }
    }
}

impl Default for EncapsulationNode {
    fn default() -> Self {
        Self::new()
    }
}

/// A method type node (for @method)
#[derive(Debug, Clone)]
pub struct MethodNode {
    pub value: String,
    pub children: Vec<ParseTree>,
    pub terminated: bool,
}

impl MethodNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
            terminated: false,
        }
    }
}

/// A method parameter node
#[derive(Debug, Clone)]
pub struct MethodParamNode {
    pub name: String,
    pub children: Vec<ParseTree>,
    pub byref: bool,
    pub variadic: bool,
    pub default: String,
}

impl MethodParamNode {
    pub fn new(name: impl Into<String>, byref: bool, variadic: bool) -> Self {
        Self {
            name: name.into(),
            children: Vec::new(),
            byref,
            variadic,
            default: String::new(),
        }
    }
}

/// Template "as" node
#[derive(Debug, Clone)]
pub struct TemplateAsNode {
    pub param_name: String,
    pub as_type: String,
    pub children: Vec<ParseTree>,
}

impl TemplateAsNode {
    pub fn new(param_name: impl Into<String>, as_type: impl Into<String>) -> Self {
        Self {
            param_name: param_name.into(),
            as_type: as_type.into(),
            children: Vec::new(),
        }
    }
}

/// Template "is" node
#[derive(Debug, Clone)]
pub struct TemplateIsNode {
    pub param_name: String,
    pub children: Vec<ParseTree>,
}

impl TemplateIsNode {
    pub fn new(param_name: impl Into<String>) -> Self {
        Self {
            param_name: param_name.into(),
            children: Vec::new(),
        }
    }
}

/// Conditional type node
#[derive(Debug, Clone)]
pub struct ConditionalNode {
    pub condition: Box<ParseTree>,
    pub children: Vec<ParseTree>,
}

impl ConditionalNode {
    pub fn new(condition: ParseTree) -> Self {
        Self {
            condition: Box::new(condition),
            children: Vec::new(),
        }
    }
}

/// Indexed access node like `T[K]`
#[derive(Debug, Clone)]
pub struct IndexedAccessNode {
    pub value: String,
    pub children: Vec<ParseTree>,
}

impl IndexedAccessNode {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            children: Vec::new(),
        }
    }
}

impl ParseTree {
    pub fn children(&self) -> &[ParseTree] {
        match self {
            ParseTree::Root(n) => &n.children,
            ParseTree::Value(_) => &[],
            ParseTree::Generic(n) => &n.children,
            ParseTree::Union(n) => &n.children,
            ParseTree::Intersection(n) => &n.children,
            ParseTree::Nullable(n) => &n.children,
            ParseTree::KeyedArray(n) => &n.children,
            ParseTree::KeyedArrayProperty(n) => &n.children,
            ParseTree::Callable(n) => &n.children,
            ParseTree::CallableWithReturnType(n) => &n.children,
            ParseTree::CallableParam(n) => &n.children,
            ParseTree::Encapsulation(n) => &n.children,
            ParseTree::Method(n) => &n.children,
            ParseTree::MethodWithReturnType(n) => &n.children,
            ParseTree::MethodParam(n) => &n.children,
            ParseTree::TemplateAs(n) => &n.children,
            ParseTree::TemplateIs(n) => &n.children,
            ParseTree::Conditional(n) => &n.children,
            ParseTree::IndexedAccess(n) => &n.children,
            ParseTree::FieldEllipsis(n) => &n.children,
        }
    }

    pub fn children_mut(&mut self) -> &mut Vec<ParseTree> {
        match self {
            ParseTree::Root(n) => &mut n.children,
            ParseTree::Value(_) => panic!("Value nodes have no children"),
            ParseTree::Generic(n) => &mut n.children,
            ParseTree::Union(n) => &mut n.children,
            ParseTree::Intersection(n) => &mut n.children,
            ParseTree::Nullable(n) => &mut n.children,
            ParseTree::KeyedArray(n) => &mut n.children,
            ParseTree::KeyedArrayProperty(n) => &mut n.children,
            ParseTree::Callable(n) => &mut n.children,
            ParseTree::CallableWithReturnType(n) => &mut n.children,
            ParseTree::CallableParam(n) => &mut n.children,
            ParseTree::Encapsulation(n) => &mut n.children,
            ParseTree::Method(n) => &mut n.children,
            ParseTree::MethodWithReturnType(n) => &mut n.children,
            ParseTree::MethodParam(n) => &mut n.children,
            ParseTree::TemplateAs(n) => &mut n.children,
            ParseTree::TemplateIs(n) => &mut n.children,
            ParseTree::Conditional(n) => &mut n.children,
            ParseTree::IndexedAccess(n) => &mut n.children,
            ParseTree::FieldEllipsis(n) => &mut n.children,
        }
    }

    pub fn is_terminated(&self) -> bool {
        match self {
            ParseTree::Generic(n) => n.terminated,
            ParseTree::KeyedArray(n) => n.terminated,
            ParseTree::Callable(n) => n.terminated,
            ParseTree::Encapsulation(n) => n.terminated,
            ParseTree::Method(n) => n.terminated,
            ParseTree::Root(n) => n.terminated,
            ParseTree::Union(n) => n.terminated,
            ParseTree::Intersection(n) => n.terminated,
            ParseTree::Nullable(n) => n.terminated,
            ParseTree::CallableWithReturnType(n) => n.terminated,
            ParseTree::MethodWithReturnType(n) => n.terminated,
            ParseTree::FieldEllipsis(n) => n.terminated,
            _ => false,
        }
    }

    pub fn set_terminated(&mut self, value: bool) {
        match self {
            ParseTree::Generic(n) => n.terminated = value,
            ParseTree::KeyedArray(n) => n.terminated = value,
            ParseTree::Callable(n) => n.terminated = value,
            ParseTree::Encapsulation(n) => n.terminated = value,
            ParseTree::Method(n) => n.terminated = value,
            ParseTree::Root(n) => n.terminated = value,
            ParseTree::Union(n) => n.terminated = value,
            ParseTree::Intersection(n) => n.terminated = value,
            ParseTree::Nullable(n) => n.terminated = value,
            ParseTree::CallableWithReturnType(n) => n.terminated = value,
            ParseTree::MethodWithReturnType(n) => n.terminated = value,
            ParseTree::FieldEllipsis(n) => n.terminated = value,
            _ => {}
        }
    }
}
