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
    /// Whether `readonly` came from the native modifier (vs a docblock
    /// `@readonly`). Psalm's readonly-default check (`$stmt->isReadonly()`)
    /// applies only to the native form.
    #[serde(default)]
    pub is_readonly_native: bool,
    pub readonly_allow_private_mutation: bool,
    pub has_default: bool,
    pub is_promoted: bool,
    /// Property hooks (`get`/`set` blocks) make the property virtual — it
    /// needs no constructor initialization.
    #[serde(default)]
    pub is_hooked: bool,
    pub is_deprecated: bool,
    /// True for properties conjured without a source location (Psalm's
    /// PropertyMap entries get a bare `PropertyStorage` with `location`
    /// null); the initialization checks skip location-less properties.
    #[serde(default)]
    pub location_free: bool,
    /// A `@psalm-suppress PropertyNotSetInConstructor` on the property's own
    /// docblock marks it initialized at scan time (Psalm's
    /// `ClassLikeNodeScanner` puts it in `initialized_properties`), exempting
    /// it - and every inheritor - from the initialization checks.
    #[serde(default)]
    pub marked_initialized: bool,
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
