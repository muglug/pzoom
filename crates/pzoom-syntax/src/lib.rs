//! PHP syntax parsing for pzoom.
//!
//! This crate wraps the mago PHP parser and provides utilities for
//! extracting class, function, and constant declarations into pzoom's
//! type system.

pub mod declaration_collector;
pub mod docblock;
pub mod name_resolver;
pub mod type_resolver;

pub use declaration_collector::DeclarationCollector;
pub use name_resolver::{resolve_names, ResolvedNames};
pub use type_resolver::resolve_hint;

// Re-export mago types that consumers need
pub use bumpalo::Bump;
pub use mago_database::file::{File, FileId};
pub use mago_span::{HasSpan, Position, Span};
pub use mago_syntax::ast::{Program, Statement};
pub use mago_syntax::parser::{parse_file, parse_file_content};
