//! Statements analyzer - orchestrates analysis of statement sequences.
//!
//! This is a lightweight wrapper that provides context for analyzing statements.

use pzoom_code_info::{
    CodebaseInfo, FunctionLikeInfo, InlineCallableTypeAnnotation, InlineTraceAnnotation,
    InlineVarTypeAnnotation,
};
use pzoom_str::{Interner, StrId};
use pzoom_syntax::ResolvedNames;

use crate::config::Config;

/// Analyzer context for a sequence of statements.
///
/// Provides access to codebase information and the current function context.
pub struct StatementsAnalyzer<'a> {
    /// Reference to the codebase for symbol lookup.
    pub codebase: &'a CodebaseInfo,

    /// Reference to the string interner.
    pub interner: &'a Interner,

    /// The function being analyzed (if any).
    pub function_info: Option<&'a FunctionLikeInfo>,

    /// The file path being analyzed.
    pub file_path: StrId,

    /// The source code being analyzed.
    pub source: &'a str,

    /// Resolved names from preprocessing (offset -> resolved StrId).
    pub resolved_names: &'a ResolvedNames,

    /// Analyzer configuration.
    pub config: &'a Config,

    /// The parse arena, when available. Used to synthesize AST nodes during analysis
    /// (e.g. rewriting a statement-level `A && B` to `if (A) { B; }`, mirroring
    /// Psalm's AndAnalyzer from_stmt path).
    pub arena: Option<&'a bumpalo::Bump>,

    /// The already-parsed top-level statements of `file_path`, when available
    /// (set by `FileAnalyzer` from its own parse). Lets the constructor
    /// property-init re-run (`init_collector`) reuse this AST for a *same-file*
    /// method body instead of re-parsing the whole file — the AST and `arena`
    /// outlive the entire `analyze_stmts` call, so the borrow is safe.
    pub file_program: Option<&'a [mago_syntax::ast::ast::statement::Statement<'a>]>,

    /// Byte offset of each line start (`[0]` is 0), built once per file so
    /// line/column lookups are a binary search instead of an O(file) scan.
    line_starts: std::rc::Rc<Vec<u32>>,

    /// Whether the file opens with `declare(strict_types=1)`, computed once
    /// per file (the argument analyzer consults this per argument).
    pub file_uses_strict_types: bool,

    /// Whether this analyzer runs a closure/arrow-function body. Closures
    /// clone the enclosing function's info (name included), so checks keyed
    /// on the function NAME (e.g. returning a value from `__construct`) must
    /// not fire inside them.
    pub inside_closure: bool,
}

impl<'a> StatementsAnalyzer<'a> {
    pub fn new(
        codebase: &'a CodebaseInfo,
        interner: &'a Interner,
        file_path: StrId,
        source: &'a str,
        resolved_names: &'a ResolvedNames,
        config: &'a Config,
    ) -> Self {
        let mut line_starts = Vec::with_capacity(source.len() / 32 + 1);
        line_starts.push(0u32);
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index as u32 + 1);
            }
        }

        // Same predicate callable_validation previously evaluated per call:
        // the first 512 chars, whitespace removed, contain the declare.
        let file_uses_strict_types = source
            .chars()
            .take(512)
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains("declare(strict_types=1)");

        Self {
            codebase,
            interner,
            function_info: None,
            file_path,
            source,
            resolved_names,
            config,
            arena: None,
            file_program: None,
            line_starts: std::rc::Rc::new(line_starts),
            file_uses_strict_types,
            inside_closure: false,
        }
    }

    /// Record the file's already-parsed top-level statements so a same-file
    /// constructor-init re-run can reuse them instead of re-parsing.
    pub fn with_file_program(
        mut self,
        statements: &'a [mago_syntax::ast::ast::statement::Statement<'a>],
    ) -> Self {
        self.file_program = Some(statements);
        self
    }

    pub fn with_function(mut self, function_info: &'a FunctionLikeInfo) -> Self {
        self.function_info = Some(function_info);
        self
    }

    /// Build a child analyzer that shares this analyzer's codebase/source/config
    /// context but runs in the scope of a different function. The child borrows
    /// `function_info` for its own (possibly shorter) lifetime `'b`, which is
    /// needed when the function info is a locally-owned value (closures, arrow
    /// functions, synthesized methods) rather than living for `'a`. A plain
    /// `with_function` cannot express that shorter borrow.
    pub fn for_nested_function<'b>(
        &self,
        function_info: Option<&'b FunctionLikeInfo>,
    ) -> StatementsAnalyzer<'b>
    where
        'a: 'b,
    {
        StatementsAnalyzer {
            codebase: self.codebase,
            interner: self.interner,
            function_info,
            file_path: self.file_path,
            source: self.source,
            resolved_names: self.resolved_names,
            config: self.config,
            arena: self.arena,
            file_program: self.file_program,
            line_starts: std::rc::Rc::clone(&self.line_starts),
            file_uses_strict_types: self.file_uses_strict_types,
            inside_closure: false,
        }
    }

    pub fn with_arena(mut self, arena: &'a bumpalo::Bump) -> Self {
        self.arena = Some(arena);
        self
    }

    /// Get the expected return type for the current function.
    pub fn get_expected_return_type(&self) -> Option<&pzoom_code_info::TUnion> {
        self.function_info.and_then(|f| f.get_return_type())
    }

    /// Check if we're analyzing a static method.
    pub fn is_static(&self) -> bool {
        self.function_info.is_some_and(|f| f.is_static)
    }

    /// Get the declaring class if this is a method.
    pub fn get_declaring_class(&self) -> Option<StrId> {
        self.function_info.and_then(|f| f.declaring_class)
    }

    /// Get a substring of the source by byte range.
    pub fn get_source_substring(&self, start: usize, end: usize) -> &str {
        &self.source[start..end.min(self.source.len())]
    }

    /// Get the line number (1-indexed) for a byte offset.
    pub fn get_line_number(&self, offset: u32) -> u32 {
        let offset = offset.min(self.source.len() as u32);
        // partition_point counts line starts at or before `offset`; the count
        // is exactly the 1-indexed line number (line_starts[0] is always 0).
        self.line_starts.partition_point(|&start| start <= offset) as u32
    }

    /// Get the column number (1-indexed) for a byte offset.
    pub fn get_column_number(&self, offset: u32) -> u32 {
        let offset = offset.min(self.source.len() as u32);
        let line_index = self.line_starts.partition_point(|&start| start <= offset) - 1;
        offset - self.line_starts[line_index] + 1
    }

    /// Get both line and column for a byte offset.
    pub fn get_line_column(&self, offset: u32) -> (u32, u32) {
        let offset = offset.min(self.source.len() as u32);
        let line_index = self.line_starts.partition_point(|&start| start <= offset) - 1;
        (
            line_index as u32 + 1,
            offset - self.line_starts[line_index] + 1,
        )
    }

    /// Look up a resolved name by its AST node offset.
    pub fn get_resolved_name(&self, offset: u32) -> Option<StrId> {
        self.resolved_names.get(&offset).copied()
    }

    /// Get preprocessed inline `@var` annotations for an expression offset.
    pub fn get_inline_var_annotations(&self, offset: u32) -> Option<&Vec<InlineVarTypeAnnotation>> {
        self.codebase
            .files
            .get(&self.file_path)
            .and_then(|file| file.inline_annotations.var_annotations.get(&offset))
    }

    /// Get preprocessed inline callable (`@param`/`@return`) annotation for an offset.
    pub fn get_inline_callable_annotation(
        &self,
        offset: u32,
    ) -> Option<&InlineCallableTypeAnnotation> {
        self.codebase
            .files
            .get(&self.file_path)
            .and_then(|file| file.inline_annotations.callable_annotations.get(&offset))
    }

    /// Get preprocessed inline `@psalm-trace` annotations for a statement/expression offset.
    pub fn get_inline_trace_annotations(&self, offset: u32) -> Option<&Vec<InlineTraceAnnotation>> {
        self.codebase
            .files
            .get(&self.file_path)
            .and_then(|file| file.inline_annotations.trace_annotations.get(&offset))
    }

    /// Get the `@psalm-scope-this` class for a statement offset.
    pub fn get_inline_scope_this_annotation(&self, offset: u32) -> Option<StrId> {
        self.codebase
            .files
            .get(&self.file_path)
            .and_then(|file| file.inline_annotations.scope_this_annotations.get(&offset))
            .copied()
    }
}

/// Error type for analysis failures.
#[derive(Debug, Clone)]
pub enum AnalysisError {
    /// User code has a fatal error that prevents further analysis.
    UserError,
    /// Internal analyzer bug - something unexpected happened.
    InternalError(String, u32, u32), // message, start_offset, end_offset
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalysisError::UserError => write!(f, "User error"),
            AnalysisError::InternalError(msg, start, end) => {
                write!(f, "Internal error at {}-{}: {}", start, end, msg)
            }
        }
    }
}

impl std::error::Error for AnalysisError {}
