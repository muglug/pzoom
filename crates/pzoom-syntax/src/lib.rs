//! PHP syntax parsing for pzoom.
//!
//! This crate wraps the mago PHP parser and provides utilities for
//! extracting class, function, and constant declarations into pzoom's
//! type system.

pub mod declaration_collector;
pub mod docblock;
pub mod name_resolver;
mod property_map;
pub mod type_resolver;

pub use declaration_collector::DeclarationCollector;
pub use name_resolver::{ResolvedNames, resolve_names};
pub use type_resolver::resolve_hint;

// Re-export mago types that consumers need
pub use mago_allocator::LocalArena;
pub use mago_database::file::{File, FileId};
pub use mago_span::{HasSpan, Position, Span};
pub use mago_syntax::cst::{Program, Statement};
pub use mago_syntax::parser::{parse_file, parse_file_content};

/// Convert a mago source-text byte slice into a `&str` borrowing the same data.
///
/// Mago 1.30 switched its AST/CST value fields (identifier names, literal text,
/// variable names, ...) from `&str` to `&'arena [u8]` slices into the file's
/// source text. pzoom interns and compares these as UTF-8 strings, so we convert
/// at the boundary. PHP source mago hands back is already treated as UTF-8; on the
/// unexpected event that a slice is not valid UTF-8 we fall back to an unchecked
/// view (matching mago's previous behaviour of exposing source bytes as `&str`)
/// rather than allocating or panicking, preserving the borrow's lifetime.
#[inline]
#[must_use]
pub fn bytes_to_str(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        // SAFETY: the bytes originate from a PHP source file that mago lexes as
        // UTF-8; this mirrors mago's prior `&str` value fields over the same data.
        Err(_) => unsafe { std::str::from_utf8_unchecked(bytes) },
    }
}
