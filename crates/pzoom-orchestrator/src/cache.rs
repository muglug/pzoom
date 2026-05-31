//! Analysis cache (stub).
//!
//! Mirrors Hakana `orchestrator/cache.rs` and Psalm `Cache.php`: persists the
//! scanned [`CodebaseInfo`] and the string [`Interner`] between runs so a
//! subsequent run can skip re-scanning unchanged files (paired with the
//! [`super::ast_differ`] diff to decide what to reuse).
//!
//! NOT YET IMPLEMENTED: pzoom has no on-disk cache or serialization layer (there
//! is no `serde`/`bincode` dependency yet). These functions define the intended
//! API; the loaders currently always report a cache miss and the writer is a
//! no-op, so analysis behaves exactly as it does today.

use std::path::Path;

use pzoom_code_info::CodebaseInfo;
use pzoom_str::Interner;

/// Load a previously serialized codebase from `path`, when caching is enabled.
///
/// TODO: implement deserialization (requires a serialization format for
/// `CodebaseInfo`). Always returns `None` (cache miss) today.
pub fn load_cached_codebase(_path: &Path, _use_cache: bool) -> Option<CodebaseInfo> {
    None
}

/// Load a previously serialized string interner from `path`, when caching is
/// enabled. The interner must be restored alongside the codebase because all
/// `StrId`s are indices into it.
///
/// TODO: implement deserialization. Always returns `None` (cache miss) today.
pub fn load_cached_interner(_path: &Path, _use_cache: bool) -> Option<Interner> {
    None
}

/// Persist `codebase` and `interner` to `path` for reuse on the next run.
///
/// TODO: implement serialization. Currently a no-op.
pub fn store_cache(_path: &Path, _codebase: &CodebaseInfo, _interner: &Interner) {}
