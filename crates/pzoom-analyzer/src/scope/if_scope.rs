//! IfScope - tracks state for if statement analysis.

use std::collections::BTreeMap;
use std::rc::Rc;

use pzoom_code_info::algebra::Clause;
use pzoom_code_info::{Assertion, TUnion};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::stmt::control_analyzer::ControlAction;

/// Scope tracking for if statement analysis.
///
/// This tracks variables that are new, redefined, removed, or possibly
/// assigned across the branches of an if statement.
#[derive(Clone, Debug, Default)]
pub struct IfScope {
    /// New variables that definitely exist after the if.
    pub new_vars: Option<BTreeMap<StrId, TUnion>>,

    /// Variables that might be in scope after the if.
    pub new_vars_possibly_in_scope: FxHashSet<StrId>,

    /// Variables that were redefined in the if.
    pub redefined_vars: Option<FxHashMap<StrId, TUnion>>,

    /// Variables that were removed in the if.
    pub removed_var_ids: FxHashSet<StrId>,

    /// Variables assigned in the if with their assignment counts.
    pub assigned_var_ids: Option<FxHashMap<StrId, usize>>,

    /// Variables that might have been assigned.
    pub possibly_assigned_var_ids: FxHashSet<StrId>,

    /// Variables that might have been redefined.
    pub possibly_redefined_vars: FxHashMap<StrId, TUnion>,

    /// Variables that were updated.
    pub updated_vars: FxHashSet<StrId>,

    /// Negated type assertions for the else branch.
    pub negated_types: BTreeMap<String, Vec<Vec<Assertion>>>,

    /// Variable IDs changed by the if condition.
    pub if_cond_changed_var_ids: FxHashSet<StrId>,

    /// Negated clauses (the if condition was false).
    pub negated_clauses: Vec<Clause>,

    /// Clauses that could be applied after the if statement,
    /// if the if statement contains branches with leaving statements,
    /// and the else leaves too.
    pub reasonable_clauses: Vec<Rc<Clause>>,

    /// Final control actions from all branches.
    pub final_actions: FxHashSet<ControlAction>,

    /// Control actions from the if branch.
    pub if_actions: FxHashSet<ControlAction>,
}
