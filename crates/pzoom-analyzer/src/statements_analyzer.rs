//! Statements analyzer - orchestrates analysis of statement sequences.
//!
//! This is a lightweight wrapper that provides context for analyzing statements.

use pzoom_code_info::{CodebaseInfo, FunctionLikeInfo};
use pzoom_str::{Interner, StrId};
use pzoom_syntax::ResolvedNames;

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
}

impl<'a> StatementsAnalyzer<'a> {
    pub fn new(
        codebase: &'a CodebaseInfo,
        interner: &'a Interner,
        file_path: StrId,
        source: &'a str,
        resolved_names: &'a ResolvedNames,
    ) -> Self {
        Self {
            codebase,
            interner,
            function_info: None,
            file_path,
            source,
            resolved_names,
        }
    }

    pub fn with_function(mut self, function_info: &'a FunctionLikeInfo) -> Self {
        self.function_info = Some(function_info);
        self
    }

    /// Get the expected return type for the current function.
    pub fn get_expected_return_type(&self) -> Option<&pzoom_code_info::TUnion> {
        self.function_info.and_then(|f| f.return_type.as_ref())
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
        let offset = offset as usize;
        self.source[..offset.min(self.source.len())]
            .bytes()
            .filter(|&b| b == b'\n')
            .count() as u32
            + 1
    }

    /// Get the column number (1-indexed) for a byte offset.
    pub fn get_column_number(&self, offset: u32) -> u32 {
        let offset = offset as usize;
        let source_prefix = &self.source[..offset.min(self.source.len())];
        // Find the last newline before this offset
        let last_newline = source_prefix.rfind('\n').map_or(0, |pos| pos + 1);
        (offset - last_newline) as u32 + 1
    }

    /// Get both line and column for a byte offset.
    pub fn get_line_column(&self, offset: u32) -> (u32, u32) {
        (self.get_line_number(offset), self.get_column_number(offset))
    }

    /// Look up a resolved name by its AST node offset.
    pub fn get_resolved_name(&self, offset: u32) -> Option<StrId> {
        self.resolved_names.get(&offset).copied()
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
