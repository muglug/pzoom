//! Type comparison result.
//!
//! Tracks detailed information about type comparisons including coercion.

use pzoom_code_info::TUnion;

/// Result of a type comparison operation.
///
/// Tracks not just whether types are compatible, but also information
/// about coercion and type mismatches for better error messages.
#[derive(Debug, Default)]
pub struct TypeComparisonResult {
    /// Whether the type was coerced (implicit conversion needed).
    pub type_coerced: Option<bool>,

    /// Whether the type was coerced from a nested mixed type.
    pub type_coerced_from_nested_mixed: Option<bool>,

    /// Whether the type was coerced from a literal to a broader type.
    pub type_coerced_to_literal: Option<bool>,

    /// Whether scalar type matching was found.
    pub scalar_type_match_found: bool,

    /// Replacement union type (for template resolution).
    pub replacement_union_type: Option<TUnion>,

    /// Whether a to_string cast would make the types compatible.
    pub to_string_cast: bool,
}

impl TypeComparisonResult {
    pub fn new() -> Self {
        Self::default()
    }
}
