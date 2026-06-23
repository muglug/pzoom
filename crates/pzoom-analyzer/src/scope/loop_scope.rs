//! Loop scope tracking for the loop fixpoint analyzer.
//!
//! Mirrors Hakana's `LoopScope` (and Psalm's `LoopScope`). It accumulates, across
//! the fixpoint iterations of a loop body, the variables that may be redefined by
//! the loop and the control-flow actions (break/continue) the body performs.
//!
//! pzoom stores locals as [`TUnion`] (not `Rc<TUnion>`) keyed by interned [`StrId`],
//! so the maps here use those types directly.

use pzoom_code_info::TUnion;
use pzoom_code_info::VarName;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::context::Locals;
use crate::stmt::scope_analyzer::ControlAction;

/// State threaded through the analysis of a loop body.
#[derive(Clone, Debug)]
pub struct LoopScope {
    /// How many times the loop body has been (re-)analyzed so far.
    pub iteration_count: usize,

    /// The locals as they were in the parent context before the loop.
    pub parent_context_vars: Locals,

    /// Variables definitely redefined on every path through the loop body.
    pub redefined_loop_vars: FxHashMap<VarName, TUnion>,

    /// Variables possibly redefined somewhere in the loop body (combined types).
    pub possibly_redefined_loop_vars: FxHashMap<VarName, TUnion>,

    /// Variables possibly redefined that were already present in the parent scope.
    pub possibly_redefined_loop_parent_vars: FxHashMap<VarName, TUnion>,

    /// Variables possibly newly-defined that should be visible (as possibly-defined)
    /// in the parent scope after the loop.
    pub possibly_defined_loop_parent_vars: FxHashMap<VarName, TUnion>,

    /// Variables that might be in scope after the loop because a leaving branch
    /// (e.g. `if (...) break;`) inside the body could define them. Mirrors Psalm's
    /// `LoopScope::$vars_possibly_in_scope`; folded into the parent's
    /// possibly-defined set once the loop finishes.
    pub vars_possibly_in_scope: FxHashSet<VarName>,

    /// The set of control-flow actions performed by the loop body (break/continue/…).
    pub final_actions: FxHashSet<ControlAction>,

    /// Variables assigned by the loop construct itself (for-init/increment
    /// counters). A nested foreach reassigning one reports LoopInvalidation
    /// (Psalm's protected_var_ids).
    pub protected_var_ids: FxHashSet<VarName>,
}

impl LoopScope {
    pub fn new(parent_context_vars: Locals) -> Self {
        Self {
            iteration_count: 0,
            parent_context_vars,
            redefined_loop_vars: FxHashMap::default(),
            possibly_redefined_loop_vars: FxHashMap::default(),
            possibly_redefined_loop_parent_vars: FxHashMap::default(),
            possibly_defined_loop_parent_vars: FxHashMap::default(),
            vars_possibly_in_scope: FxHashSet::default(),
            final_actions: FxHashSet::default(),
            protected_var_ids: FxHashSet::default(),
        }
    }
}
