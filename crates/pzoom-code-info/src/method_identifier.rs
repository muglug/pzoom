//! Method identifier.
//!
//! Mirrors Hakana's `code_info/method_identifier.rs` and Psalm's
//! `MethodIdentifier`: a `(fully-qualified class id, method-name id)` pair used
//! to refer to a method throughout resolution and data-flow tracking.

use pzoom_str::{Interner, StrId};
use serde::{Deserialize, Serialize};

use crate::data_flow::node::lookup_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash, PartialOrd, Ord)]
pub struct MethodIdentifier(pub StrId, pub StrId);

impl MethodIdentifier {
    pub fn to_string(&self, interner: &Interner) -> String {
        format!(
            "{}::{}",
            lookup_id(interner, self.0),
            lookup_id(interner, self.1)
        )
    }
}
