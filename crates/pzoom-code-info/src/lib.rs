//! Type system and codebase data structures for pzoom.
//!
//! This crate defines the core type system modeled after Psalm's Union/Atomic
//! pattern, along with storage structures for classes, functions, and files.

pub mod algebra;
pub mod assertion;
pub mod t_atomic;
pub mod t_union;
pub mod ttype;
pub mod codebase_info;
pub mod class_like_info;
pub mod functionlike_info;
pub mod issue;
pub mod symbol;

pub use algebra::{Clause, ClauseKey};
pub use assertion::Assertion;
pub use t_atomic::{TAtomic, FunctionLikeParameter, ArrayKey};
pub use t_union::TUnion;
pub use ttype::{combine_union_types, add_union_type};
pub use codebase_info::CodebaseInfo;
pub use class_like_info::ClassLikeInfo;
pub use functionlike_info::FunctionLikeInfo;
pub use issue::{Issue, IssueKind};
pub use symbol::SymbolKind;
