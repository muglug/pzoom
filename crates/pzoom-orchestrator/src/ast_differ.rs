//! AST differ (stub).
//!
//! Mirrors Hakana `orchestrator/ast_differ.rs` and Psalm `AstDiffer.php`. The job
//! of this module is to compute which top-level definitions changed between two
//! scans so that a future incremental mode can re-analyze only the affected files
//! instead of the whole codebase (paired with [`super::cache`]).
//!
//! NOT YET IMPLEMENTED: pzoom currently re-scans and re-analyzes every file on
//! each run. This module only defines the intended API surface; [`get_diff`]
//! conservatively reports "no reusable information", which callers must treat as
//! "re-analyze everything".

use pzoom_code_info::codebase_info::FileInfo;
use pzoom_syntax::FileId;
use rustc_hash::{FxHashMap, FxHashSet};

/// Description of how the codebase changed between two scans.
///
/// Mirrors Hakana's `CodebaseDiff`: definitions whose bodies are unchanged
/// (`keep`), definitions whose signatures are unchanged but bodies may differ
/// (`keep_signature`), and definitions added or removed (`add_or_delete`).
#[derive(Debug, Default)]
pub struct CodebaseDiff {
    /// Definitions present and structurally unchanged in both scans.
    pub keep: FxHashSet<FileId>,
    /// Definitions whose signature is unchanged (body may differ).
    pub keep_signature: FxHashSet<FileId>,
    /// Definitions added in the new scan or removed from the old scan.
    pub add_or_delete: FxHashSet<FileId>,
}

/// Compute the diff between a previous scan and a new scan.
///
/// TODO: implement the AST-signature diff used by Hakana/Psalm (a Myers-style LCS
/// over each file's top-level definition signatures, propagating dependents via
/// the symbol-reference graph). Until then this returns an empty diff, which
/// callers must interpret as "everything changed".
pub fn get_diff(
    _existing_files: &FxHashMap<FileId, FileInfo>,
    _new_files: &FxHashMap<FileId, FileInfo>,
) -> CodebaseDiff {
    CodebaseDiff::default()
}
