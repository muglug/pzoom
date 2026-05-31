//! Property information.
//!
//! Mirrors Hakana's `property_info.rs` (and Psalm's `PropertyStorage`): stores
//! both the native PHP type hint (`signature_type`) and the docblock type
//! (`property_type`). Split out of [`crate::class_like_info`].

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::TUnion;
use crate::member_visibility::Visibility;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyInfo {
    pub name: StrId,
    pub declaring_class: StrId,
    /// The effective type for analysis (docblock type if present, else signature type).
    pub property_type: Option<TUnion>,
    /// The native PHP type hint (from property declaration).
    pub signature_type: Option<TUnion>,
    pub visibility: Visibility,
    pub is_static: bool,
    pub is_readonly: bool,
    pub readonly_allow_private_mutation: bool,
    pub has_default: bool,
    pub is_promoted: bool,
    pub is_deprecated: bool,
    /// Internal visibility scopes for this property (`@internal` / `@psalm-internal`).
    /// Empty means the property is publicly accessible.
    pub internal: Vec<StrId>,
    pub description: Option<String>,
    pub start_offset: u32,
}

impl PropertyInfo {
    /// Get the effective type for analysis: the docblock `property_type` if present,
    /// otherwise the native `signature_type`. Mirrors Psalm's `type ?: signature_type`
    /// while keeping the two stored separately.
    pub fn get_type(&self) -> Option<&TUnion> {
        self.property_type.as_ref().or(self.signature_type.as_ref())
    }

    /// Check if this property has an explicit type declaration (either signature or docblock).
    pub fn has_type(&self) -> bool {
        self.property_type.is_some() || self.signature_type.is_some()
    }
}
