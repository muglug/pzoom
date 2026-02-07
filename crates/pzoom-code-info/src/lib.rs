//! Type system and codebase data structures for pzoom.
//!
//! This crate defines the core type system modeled after Psalm's Union/Atomic
//! pattern, along with storage structures for classes, functions, and files.

pub mod algebra;
pub mod assertion;
pub mod class_like_info;
pub mod codebase_info;
pub mod data_flow;
pub mod functionlike_info;
pub mod issue;
pub mod symbol;
pub mod t_atomic;
pub mod t_union;
pub mod ttype;

pub use algebra::{Clause, ClauseKey};
pub use assertion::Assertion;
pub use class_like_info::ClassLikeInfo;
pub use codebase_info::{
    CodebaseInfo, InlineCallableParamType, InlineCallableTypeAnnotation, InlineTraceAnnotation,
    InlineTypeAnnotations, InlineVarTypeAnnotation,
};
pub use data_flow::{
    graph::{DataFlowGraph, GraphKind, WholeProgramKind},
    node::{
        DataFlowNode, DataFlowNodeId, DataFlowNodePosition, FunctionLikeIdentifier,
        MethodIdentifier, VarId, VariableSourceKind,
    },
    path::PathKind,
};
pub use functionlike_info::FunctionLikeInfo;
pub use issue::{Issue, IssueKind};
pub use symbol::SymbolKind;
pub use t_atomic::{ArrayKey, FunctionLikeParameter, TAtomic};
pub use t_union::TUnion;
pub use ttype::{add_union_type, combine_union_types};
