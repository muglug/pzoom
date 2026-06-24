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
/// at the boundary. PHP source is overwhelmingly UTF-8, but a string literal can
/// carry arbitrary bytes (e.g. `"\xD0\xCF..."` binary data). To stay memory-safe
/// — an invalid `&str` is undefined behaviour the moment anything walks its char
/// boundaries — we never produce an unchecked `&str`: valid input is returned
/// as-is, and otherwise we return the longest valid UTF-8 prefix (empty when the
/// first byte is already invalid). This borrows the same data with no allocation.
#[inline]
#[must_use]
pub fn bytes_to_str(bytes: &[u8]) -> &str {
    match std::str::from_utf8(bytes) {
        Ok(s) => s,
        // The slice up to `valid_up_to()` is guaranteed valid UTF-8, so this
        // never panics and never constructs an invalid `&str`.
        Err(error) => std::str::from_utf8(&bytes[..error.valid_up_to()]).unwrap_or(""),
    }
}
