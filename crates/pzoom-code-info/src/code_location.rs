//! Source code location.
//!
//! Mirrors Psalm's `CodeLocation` and Hakana's `code_location.rs`: the file and
//! byte/line/column span an issue (or other diagnostic) points at. pzoom tracks
//! the start line/column (resolved up front) plus the byte offsets of the span.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLocation {
    pub file_path: StrId,
    pub start_offset: u32,
    pub end_offset: u32,
    pub start_line: u32,
    pub start_column: u32,
}

impl CodeLocation {
    pub fn new(
        file_path: StrId,
        start_offset: u32,
        end_offset: u32,
        start_line: u32,
        start_column: u32,
    ) -> Self {
        Self {
            file_path,
            start_offset,
            end_offset,
            start_line,
            start_column,
        }
    }
}
