//! Per-file scanned information.
//!
//! Mirrors Hakana's `file_info.rs`: `FileInfo` records what a single scanned
//! file defines (classes, functions, constants) plus scanner-preprocessed inline
//! docblock annotations. Split out of [`crate::codebase_info`] (which keeps the
//! codebase-wide `CodebaseInfo`).

use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::TUnion;

/// Information about a scanned file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: StrId,
    /// Classes defined in this file.
    pub classes: Vec<StrId>,
    /// Functions defined in this file.
    pub functions: Vec<StrId>,
    /// Constants defined in this file.
    pub constants: Vec<StrId>,
    /// Hash of file contents for cache invalidation.
    pub content_hash: String,
    /// The file contents (for re-parsing during analysis).
    pub contents: String,
    /// Parser diagnostics for this file as (offset, message) — surfaced as
    /// ParseError issues during analysis (Psalm's parser errors become issues).
    #[serde(default)]
    pub parse_errors: Vec<(u32, String)>,
    /// Scan-time docblock problems as (offset, message) — surfaced as
    /// InvalidDocblock issues during analysis (e.g. a malformed
    /// `@psalm-type` definition).
    #[serde(default)]
    pub docblock_parse_issues: Vec<(u32, String)>,
    /// Whether this file is a stub file.
    #[serde(default)]
    pub is_stub: bool,
    /// Whether this is a *low-precedence* stub (the phpstorm-derived
    /// `stubs/extensions/*` set). Mirrors Psalm's precedence: pzoom's own curated
    /// stubs (`CoreGenericFunctions`, `Php*`, `SPL`, …) take precedence over the
    /// phpstorm-stubs, which fill in only declarations the curated stubs don't define.
    #[serde(default)]
    pub is_low_precedence_stub: bool,
    /// Whether the file is part of the analyzed project (Psalm's
    /// `Config::isInProjectDirs`). Dependency sources (vendor/) are scanned for
    /// declarations but are not project files: a stub may member-override their
    /// classes, while a project-dir class always beats the stub.
    #[serde(default = "default_true")]
    pub is_in_project_dirs: bool,
    /// Preprocessed inline docblock annotations keyed by expression/statement offset.
    #[serde(default)]
    pub inline_annotations: InlineTypeAnnotations,
    /// `@psalm-import-type ALIAS from CLASS` records (source class id, alias
    /// name), validated against the populated codebase during analysis.
    #[serde(default)]
    pub type_alias_imports: Vec<(StrId, String)>,
    /// Name resolution computed during scanning: AST node offset -> resolved
    /// fully-qualified `StrId`. Analysis reuses this map (keyed by the same
    /// offsets, since it re-parses the identical file contents) instead of
    /// re-resolving names, which would have to intern — impossible while
    /// analysis only holds a shared `&Interner`.
    #[serde(default)]
    pub resolved_names: FxHashMap<u32, StrId>,
}

fn default_true() -> bool {
    true
}

/// Scanner-preprocessed inline type annotations for a file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InlineTypeAnnotations {
    /// Inline `@var` annotations keyed by the offset of the annotated expression.
    #[serde(default)]
    pub var_annotations: FxHashMap<u32, Vec<InlineVarTypeAnnotation>>,
    /// Inline callable (`@param`/`@return`) annotations keyed by closure/arrow offset.
    #[serde(default)]
    pub callable_annotations: FxHashMap<u32, InlineCallableTypeAnnotation>,
    /// Inline `@psalm-trace` annotations keyed by statement/expression offset.
    #[serde(default)]
    pub trace_annotations: FxHashMap<u32, Vec<InlineTraceAnnotation>>,
    /// Inline `@psalm-check-type` / `@psalm-check-type-exact` annotations keyed by
    /// the offset of the statement they precede (or the docblock offset when no
    /// statement follows, for malformed annotations).
    #[serde(default)]
    pub check_type_annotations: FxHashMap<u32, Vec<InlineCheckTypeAnnotation>>,
    /// `@psalm-scope-this C` annotations keyed by the offset of the statement
    /// they precede: from that statement on, `$this` is typed as the resolved
    /// class (Psalm's StatementsAnalyzer `psalm-scope-this` handling).
    #[serde(default)]
    pub scope_this_annotations: FxHashMap<u32, StrId>,
}

/// A single inline `@psalm-check-type[-exact]` assertion (`$var = Type`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineCheckTypeAnnotation {
    /// The raw left-hand side as written, including a trailing `?` if present
    /// (e.g. "$foo" or "$foo?"). `None` when the variable is missing.
    pub checked_var_raw: Option<String>,
    /// Interned variable id (e.g. "$foo"), with any trailing `?` stripped.
    pub var_id: Option<StrId>,
    /// The asserted type, or `None` when the type string is missing/unparseable.
    pub check_type: Option<TUnion>,
    /// Whether the assertion marked the variable possibly-undefined (`$foo?`).
    pub annotation_possibly_undefined: bool,
    /// Whether this is a `@psalm-check-type-exact` (bidirectional) assertion.
    pub is_exact: bool,
}

/// A single inline `@var` annotation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineVarTypeAnnotation {
    /// Optional variable name this annotation targets (e.g. "$x").
    pub var_name: Option<StrId>,
    pub var_type: TUnion,
    #[serde(default)]
    pub is_invalid: bool,
    /// The legacy name-first form (`@var $x Type`): Psalm's CommentAnalyzer
    /// throws "Misplaced variable", reported as MissingDocblockType.
    #[serde(default)]
    pub is_misplaced_variable: bool,
}

/// Inline callable annotation data for anonymous functions.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InlineCallableTypeAnnotation {
    pub params: Vec<InlineCallableParamType>,
    pub return_type: Option<TUnion>,
    #[serde(default)]
    pub has_template_annotation: bool,
    #[serde(default)]
    pub is_pure: bool,
}

/// Inline callable parameter annotation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineCallableParamType {
    /// Optional parameter name (e.g. "$x").
    pub param_name: Option<StrId>,
    pub param_type: TUnion,
}

/// Inline trace annotation data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InlineTraceAnnotation {
    /// Variables to trace (e.g. "$x", "$y").
    pub var_names: Vec<StrId>,
}
