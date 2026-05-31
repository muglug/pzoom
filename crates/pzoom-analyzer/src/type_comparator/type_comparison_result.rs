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

    /// Whether the coercion came from a `scalar` input being narrowed to a
    /// concrete scalar (e.g. array access by arbitrary int). Mirrors Psalm's
    /// `type_coerced_from_scalar`.
    pub type_coerced_from_scalar: Option<bool>,

    /// Whether scalar type matching was found.
    ///
    /// Mirrors Psalm's `?bool $scalar_type_match_found`. `None` means
    /// "not yet determined"; the union comparator seeds it to `Some(true)` and
    /// clears it to `Some(false)` when a non-scalar mismatch is encountered.
    pub scalar_type_match_found: Option<bool>,

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
