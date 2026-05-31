//! Class constant information.
//!
//! Mirrors Hakana's `class_constant_info.rs`. Split out of
//! [`crate::class_like_info`].

use pzoom_str::StrId;
use serde::{Deserialize, Serialize};

use crate::TUnion;
use crate::member_visibility::Visibility;

/// Information about a class constant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassConstantInfo {
    pub name: StrId,
    pub declaring_class: StrId,
    pub constant_type: TUnion,
    pub visibility: Visibility,
    pub is_final: bool,
    pub is_deprecated: bool,
    pub start_offset: u32,
}
