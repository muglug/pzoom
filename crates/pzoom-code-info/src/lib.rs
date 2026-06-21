//! Type system and codebase data structures for pzoom.
//!
//! This crate defines the core type system modeled after Psalm's Union/Atomic
//! pattern, along with storage structures for classes, functions, and files.

pub mod algebra;
pub mod assertion;
pub mod attribute_info;
pub mod class_constant_info;
pub mod class_like_info;
pub mod class_type_alias;
pub mod code_location;
pub mod codebase_info;
pub mod data_flow;
pub mod file_info;
pub mod functionlike_info;
pub mod issue;
pub mod member_visibility;
pub mod method_identifier;
pub mod property_info;
pub mod runtime_constants;
pub mod symbol;
pub mod symbol_references;
pub mod t_atomic;
pub mod t_union;
pub mod ttype;
pub mod type_resolution;
pub mod var_name;

pub use algebra::{AssertionSet, Clause, ClauseKey};
pub use assertion::Assertion;
pub use attribute_info::AttributeMap;
pub use class_like_info::ClassLikeInfo;
pub use class_type_alias::ClassTypeAlias;
pub use code_location::CodeLocation;
pub use codebase_info::{
    CodebaseInfo, GlobalDefine, GlobalDefineValue, InlineCallableParamType,
    InlineCallableTypeAnnotation, InlineCheckTypeAnnotation, InlineTraceAnnotation,
    InlineTypeAnnotations, InlineVarTypeAnnotation,
};
pub use data_flow::{
    graph::{DataFlowGraph, GraphKind, WholeProgramKind},
    node::{
        DataFlowNode, DataFlowNodeId, DataFlowNodeKind, DataFlowNodePosition,
        FunctionLikeIdentifier, VarId, VariableSourceKind,
    },
    path::{ArrayDataKind, PathKind},
};
pub use functionlike_info::FunctionLikeInfo;
pub use issue::{Issue, IssueKind, SecondaryLocation, TraceNode};
pub use method_identifier::MethodIdentifier;
pub use symbol::SymbolKind;
pub use t_atomic::{ArrayKey, FunctionLikeParameter, TAtomic};
pub use t_union::TUnion;
pub use ttype::template::{GenericParent, TemplateBound, TemplateResult, TypeVariableBounds};
pub use ttype::{add_union_type, combine_union_types, combine_union_types_with_codebase};
pub use var_name::VarName;

/// Prefix of the synthetic classlike name given to anonymous classes:
/// `@anonymous-class:{file}:{offset}` (Psalm registers them as
/// `{parent}@anonymous` storages; the scanner and analyzer must agree on
/// the name to find the registered storage).
pub const ANONYMOUS_CLASS_PREFIX: &str = "@anonymous-class";
