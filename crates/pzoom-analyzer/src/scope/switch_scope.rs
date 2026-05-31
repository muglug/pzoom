//! Switch scope.
//!
//! Mirrors Hakana's `analyzer/scope/switch_scope.rs` and Psalm's `SwitchScope`:
//! the state accumulated *across* the cases of a single `switch` while it is
//! analyzed. Hakana/Psalm track variable-scope merging (new_locals,
//! redefined_vars, leftover statements); pzoom's switch analysis instead tracks
//! case exhaustiveness and fallthrough, so this scope carries the
//! contexts of fallthrough-able cases (merged at the end), the remaining
//! unmatched switch type, the seen case keys, and the pending fallthrough case
//! types.

use std::collections::BTreeMap;

use pzoom_code_info::algebra::Clause;
use pzoom_code_info::{Assertion, TUnion};

use crate::context::BlockContext;

pub struct SwitchScope {
    /// Whether a `default:` case was seen.
    pub has_default: bool,
    /// Whether every case so far exits (return/throw), so the switch does too.
    pub all_options_returned: bool,
    /// Contexts of cases that fall through to the post-switch code (break/hybrid),
    /// merged once all cases are analyzed.
    pub continuing_contexts: Vec<BlockContext>,
    /// The portion of the switch subject type not yet matched by a case
    /// (only meaningful while `can_track_remaining`).
    pub remaining_switch_type: TUnion,
    /// For `switch (true)`: the false-branch assertions accumulated from prior
    /// cases, applied to each subsequent case.
    pub accumulated_false_assertions: BTreeMap<String, Vec<Assertion>>,
    /// Case types of empty (fallthrough) cases pending the next case body.
    pub pending_fallthrough_case_types: Vec<TUnion>,
    /// CNF clauses negating each previously analyzed case's equality expression.
    /// Mirrors Psalm/Hakana `SwitchScope::negated_clauses`: a later case enters
    /// with every earlier case's `switch_cond === case_cond` known to be false,
    /// which lets the formula machinery flag impossible (already-matched) cases
    /// and narrow the switch subject as the cases are subtracted from it.
    pub negated_clauses: Vec<Clause>,
    /// The OR-combined equality clauses of the empty (fall-through) cases seen so
    /// far, pending the next case with a body. Mirrors Psalm/Hakana
    /// `SwitchScope::leftover_case_equality_expr`: `case "a": case "b": case "c":
    /// <body>` analyzes the body knowing `$x === "a" || $x === "b" || $x === "c"`,
    /// so the subject is narrowed to the whole fall-through group, not just the
    /// last label.
    pub leftover_case_equality_clauses: Option<Vec<Clause>>,
}

impl SwitchScope {
    pub fn new(switch_expr_type: TUnion) -> Self {
        Self {
            has_default: false,
            all_options_returned: true,
            continuing_contexts: Vec::new(),
            remaining_switch_type: switch_expr_type,
            accumulated_false_assertions: BTreeMap::new(),
            pending_fallthrough_case_types: Vec::new(),
            negated_clauses: Vec::new(),
            leftover_case_equality_clauses: None,
        }
    }
}
