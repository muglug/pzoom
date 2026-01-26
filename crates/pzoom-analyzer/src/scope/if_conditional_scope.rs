//! IfConditionalScope - result of analyzing an if condition.

use pzoom_str::StrId;
use rustc_hash::FxHashSet;

use crate::context::BlockContext;

/// Result of analyzing an if condition.
///
/// Contains the contexts for the if body, the post-if context,
/// and tracks which variables were referenced in the condition.
#[derive(Clone)]
pub struct IfConditionalScope {
    /// Context for the if body (condition is true).
    pub if_body_context: BlockContext,

    /// Context after the condition analysis but before the if body.
    pub outer_context: BlockContext,

    /// Context to use after the if statement.
    pub post_if_context: BlockContext,

    /// Variables referenced in the condition.
    pub cond_referenced_var_ids: FxHashSet<StrId>,
}
