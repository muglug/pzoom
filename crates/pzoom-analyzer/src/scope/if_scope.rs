//! IfScope - tracks state for if statement analysis.

use std::collections::BTreeMap;
use std::rc::Rc;

use pzoom_code_info::VarName;
use pzoom_code_info::algebra::Clause;
use pzoom_code_info::{Assertion, TUnion};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::stmt::scope_analyzer::ControlAction;

/// Scope tracking for if statement analysis.
///
/// This tracks variables that are new, redefined, removed, or possibly
/// assigned across the branches of an if statement.
#[derive(Clone, Debug, Default)]
pub struct IfScope {
    /// New variables that definitely exist after the if.
    pub new_vars: Option<BTreeMap<VarName, TUnion>>,

    /// Variables that might be in scope after the if.
    pub new_vars_possibly_in_scope: FxHashSet<VarName>,

    /// Variables that were redefined in the if.
    pub redefined_vars: Option<FxHashMap<VarName, TUnion>>,

    /// Variables that were removed in the if.
    pub removed_var_ids: FxHashSet<VarName>,

    /// Variables assigned in the if with their assignment counts.
    pub assigned_var_ids: Option<FxHashMap<VarName, usize>>,

    /// Variables that might have been assigned.
    pub possibly_assigned_var_ids: FxHashSet<VarName>,

    /// Variables that might have been redefined.
    pub possibly_redefined_vars: FxHashMap<VarName, TUnion>,

    /// Variables that were updated.
    pub updated_vars: FxHashSet<VarName>,

    /// Negated type assertions for the else branch.
    pub negated_types: BTreeMap<VarName, Vec<Vec<Assertion>>>,

    /// Variable IDs changed by the if condition.
    pub if_cond_changed_var_ids: FxHashSet<VarName>,

    /// Variables the if condition narrowed that can be negated in the fallthrough
    /// branches — the `vars_to_update` set passed to `BlockContext::update`.
    pub negatable_if_types: FxHashSet<VarName>,

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
