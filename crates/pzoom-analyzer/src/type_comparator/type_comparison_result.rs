//! Type comparison result.
//!
//! Tracks detailed information about type comparisons including coercion.

use pzoom_code_info::{TUnion, TemplateBound};

/// Result of a type comparison operation.
///
/// Tracks not just whether types are compatible, but also information
/// about coercion and type mismatches for better error messages.
#[derive(Debug, Default)]
pub struct TypeComparisonResult {
    /// Whether the type was coerced (implicit conversion needed).
    pub type_coerced: Option<bool>,

    /// Whether the coercion came from a mixed input (or a mixed generic
    /// param of it) — Psalm's `type_coerced_from_mixed`.
    pub type_coerced_from_mixed: Option<bool>,

    /// Whether the mixed origin was a template's `as mixed` bound, which
    /// suppresses Mixed* reporting — Psalm's `type_coerced_from_as_mixed`.
    pub type_coerced_from_as_mixed: Option<bool>,

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

    /// Lower bounds recorded for type variables encountered in container
    /// position (Hakana's `type_variable_lower_bounds`): `name >: input`.
    pub type_variable_lower_bounds: Vec<(String, TemplateBound)>,

    /// Upper bounds recorded for type variables encountered in input position
    /// (Hakana's `type_variable_upper_bounds`): `name <: container`.
    pub type_variable_upper_bounds: Vec<(String, TemplateBound)>,

    /// Whether a to_string cast would make the types compatible.
    pub to_string_cast: bool,
}

impl TypeComparisonResult {
    pub fn new() -> Self {
        Self::default()
    }
}
