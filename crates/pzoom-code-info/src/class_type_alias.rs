//! Class type alias.
//!
//! Mirrors Hakana's `code_info/class_type_alias.rs` and Psalm's
//! `ClassTypeAlias` (a `TypeAlias`): the type a `@psalm-type` / `@phpstan-type`
//! alias resolves to. Psalm stores the bare `replacement_atomic_types`; pzoom
//! keeps the equivalent as a resolved `aliased_type` union plus the location
//! metadata it was declared at.

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::TUnion;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassTypeAlias {
    pub name: StrId,
    /// The type the alias expands to (Psalm's `replacement_atomic_types`).
    pub aliased_type: TUnion,
    pub file_path: StrId,
    pub start_offset: u32,
}
