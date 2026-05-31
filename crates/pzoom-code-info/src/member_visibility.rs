//! Member visibility.
//!
//! Mirrors Hakana's `member_visibility.rs`: the visibility modifier shared by
//! class properties, constants, and methods. Split out of [`crate::class_like_info`].

use serde::{Deserialize, Serialize};

/// Visibility modifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Visibility {
    #[default]
    Public,
    Protected,
    Private,
}
