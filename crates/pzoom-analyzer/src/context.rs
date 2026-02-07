//! Analysis context - tracks scope state during analysis.
//!
//! Modeled after Psalm's Context and hakana's BlockContext.

use std::cell::RefCell;
use std::rc::Rc;

use pzoom_code_info::algebra::Clause;
use pzoom_code_info::{TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

/// Context for analyzing a block of code.
///
/// Tracks variable types, assignments, and scope state.
#[derive(Clone, Debug, Default)]
pub struct BlockContext {
    /// Variable types currently in scope: `$varName` -> Type.
    pub locals: FxHashMap<StrId, TUnion>,

    /// Variables that have definitely been assigned in this scope (with count).
    pub assigned_var_ids: FxHashMap<StrId, usize>,

    /// Variables that may have been assigned (e.g., in one branch of an if).
    pub possibly_assigned_var_ids: FxHashSet<StrId>,

    /// Variables referenced in conditional contexts.
    pub cond_referenced_var_ids: FxHashSet<StrId>,

    /// Active CNF clauses representing what is known to be true at this point.
    /// Used for type algebra simplification in nested conditionals.
    pub clauses: Vec<Rc<Clause>>,

    /// Clauses that were reconciled and should be removed from parent context.
    pub reconciled_expression_clauses: Vec<Rc<Clause>>,

    /// Whether control flow has returned/thrown/exited.
    /// When true, any subsequent statements are unreachable.
    pub has_returned: bool,

    /// Whether we're inside a throw expression.
    pub inside_throw: bool,

    /// Whether we're inside a conditional expression.
    pub inside_conditional: bool,

    /// Whether we're inside an isset() call.
    pub inside_isset: bool,

    /// Whether we're inside an unset() call.
    pub inside_unset: bool,

    /// Whether we're analyzing an expression in a general-use context
    /// (e.g. array offset/index computation) where Psalm suppresses certain
    /// mixed-property diagnostics.
    pub inside_general_use: bool,

    /// Whether undefined-variable checks should be emitted.
    ///
    /// Disabled after dynamic code evaluation constructs (e.g. `eval`) because
    /// variable definitions become unknown.
    pub check_variables: bool,

    /// Whether we're inside a loop.
    pub inside_loop: bool,

    /// Whether the current loop scope is a foreach body.
    pub inside_foreach: bool,

    /// Whether we're inside a try block.
    pub inside_try: bool,

    /// The current class (if any).
    pub self_class: Option<StrId>,

    /// The parent class (if any).
    pub parent_class: Option<StrId>,

    /// Whether $this is available.
    pub has_this: bool,

    /// The current namespace (if any).
    pub namespace: Option<StrId>,

    /// Tracks `$class = get_class($obj)` style relationships for class-string narrowing.
    pub class_string_origins: FxHashMap<StrId, StrId>,

    /// Maps in-scope references to the variable they reference (`$b => $a` for `$b = &$a`).
    pub references_in_scope: FxHashMap<StrId, StrId>,

    /// Set of references to values outside the current local scope (array offsets/properties/globals).
    pub references_to_external_scope: FxHashSet<StrId>,

    /// References that may have originated in a confusing scope (if/loop), and are unsafe to reuse.
    pub references_possibly_from_confusing_scope: FxHashSet<StrId>,

    /// Runtime class aliases declared via `class_alias`.
    pub class_aliases: FxHashMap<StrId, StrId>,

    /// Constants defined at analysis time via `define()`.
    pub defined_constants: FxHashMap<StrId, TUnion>,

    /// Type constraints imposed by by-ref parameters on referenced variables.
    pub reference_constraints: FxHashMap<StrId, Vec<TUnion>>,

    /// Function-local static variables declared via `static $x`.
    pub static_var_ids: FxHashSet<StrId>,

    /// Expected callable types for closure/arrow expressions keyed by expression start offset.
    pub expected_callable_arg_types: FxHashMap<u32, TUnion>,

    /// Reference to the if body context when inside a conditional.
    pub if_body_context: Option<Rc<RefCell<BlockContext>>>,

    /// Function context information.
    pub function_context: FunctionContextInfo,
}

/// Information about the current function context.
#[derive(Clone, Debug, Default)]
pub struct FunctionContextInfo {
    /// The calling class (if this is a method).
    pub calling_class: Option<StrId>,
    /// The calling functionlike identifier.
    pub calling_functionlike_id: Option<FunctionLikeId>,
}

/// Identifier for a function-like entity.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionLikeId {
    Function(StrId),
    Method(StrId, StrId), // class, method
}

impl BlockContext {
    pub fn new() -> Self {
        Self {
            check_variables: true,
            ..Self::default()
        }
    }

    /// Create a child context for a nested scope.
    pub fn child(&self) -> Self {
        Self {
            locals: self.locals.clone(),
            assigned_var_ids: FxHashMap::default(),
            possibly_assigned_var_ids: FxHashSet::default(),
            cond_referenced_var_ids: FxHashSet::default(),
            clauses: self.clauses.clone(),
            reconciled_expression_clauses: Vec::new(),
            has_returned: false, // Child scope starts fresh
            inside_throw: false,
            inside_conditional: self.inside_conditional,
            inside_isset: false,
            inside_unset: false,
            inside_general_use: self.inside_general_use,
            check_variables: self.check_variables,
            inside_loop: self.inside_loop,
            inside_foreach: self.inside_foreach,
            inside_try: self.inside_try,
            self_class: self.self_class,
            parent_class: self.parent_class,
            has_this: self.has_this,
            namespace: self.namespace,
            class_string_origins: self.class_string_origins.clone(),
            references_in_scope: self.references_in_scope.clone(),
            references_to_external_scope: self.references_to_external_scope.clone(),
            references_possibly_from_confusing_scope: self
                .references_possibly_from_confusing_scope
                .clone(),
            class_aliases: self.class_aliases.clone(),
            defined_constants: self.defined_constants.clone(),
            reference_constraints: self.reference_constraints.clone(),
            static_var_ids: self.static_var_ids.clone(),
            expected_callable_arg_types: self.expected_callable_arg_types.clone(),
            if_body_context: None,
            function_context: self.function_context.clone(),
        }
    }

    /// Get the type of a variable, if known.
    pub fn get_var_type(&self, var_id: StrId) -> Option<&TUnion> {
        self.locals.get(&var_id)
    }

    /// Set the type of a variable.
    pub fn set_var_type(&mut self, var_id: StrId, var_type: TUnion) {
        let root_var_id = self.get_reference_root(var_id);
        self.propagate_reference_cluster_type(root_var_id, var_type);
    }

    /// Update the inferred type of a variable without treating it as an assignment.
    pub fn set_var_type_for_inference(&mut self, var_id: StrId, var_type: TUnion) {
        let root_var_id = self.get_reference_root(var_id);
        self.propagate_reference_cluster_type_without_assignment(root_var_id, var_type);
    }

    /// Set the type of a variable directly, without propagating through reference bindings.
    pub fn set_var_type_direct(&mut self, var_id: StrId, var_type: TUnion) {
        self.locals.insert(var_id, var_type);
        *self.assigned_var_ids.entry(var_id).or_insert(0) += 1;
    }

    /// Check if a variable has been definitely assigned.
    pub fn is_assigned(&self, var_id: StrId) -> bool {
        self.assigned_var_ids.contains_key(&var_id)
    }

    /// Check if a variable might be assigned.
    pub fn is_possibly_assigned(&self, var_id: StrId) -> bool {
        self.possibly_assigned_var_ids.contains(&var_id)
    }

    pub fn add_reference_constraint(&mut self, var_id: StrId, constraint: TUnion) {
        let root_var_id = self.get_reference_root(var_id);
        let entry = self.reference_constraints.entry(root_var_id).or_default();
        if !entry.contains(&constraint) {
            entry.push(constraint);
        }
    }

    pub fn get_reference_constraints(&self, var_id: StrId) -> Option<&Vec<TUnion>> {
        let root_var_id = self.get_reference_root(var_id);
        self.reference_constraints.get(&root_var_id)
    }

    /// Get variables that were redefined compared to a parent context.
    pub fn get_redefined_locals(
        &self,
        parent_locals: &FxHashMap<StrId, TUnion>,
        _check_equality: bool,
        removed_vars: &mut FxHashSet<StrId>,
    ) -> FxHashMap<StrId, TUnion> {
        let mut redefined = FxHashMap::default();

        for (var_id, var_type) in &self.locals {
            if let Some(parent_type) = parent_locals.get(var_id) {
                // Variable exists in both - check if redefined
                if var_type != parent_type {
                    redefined.insert(*var_id, var_type.clone());
                }
            }
        }

        // Track variables that were removed
        for var_id in parent_locals.keys() {
            if !self.locals.contains_key(var_id) {
                removed_vars.insert(*var_id);
            }
        }

        redefined
    }

    /// Remove reconciled clause refs from a set of clauses.
    ///
    /// Returns the filtered clauses and a set of changed variable IDs.
    pub fn remove_reconciled_clause_refs(
        clauses: &[Rc<Clause>],
        changed_var_ids: &FxHashSet<StrId>,
        interner: &pzoom_str::Interner,
    ) -> (Vec<Rc<Clause>>, FxHashSet<StrId>) {
        use pzoom_code_info::algebra::ClauseKey;

        let mut result = Vec::new();
        let mut affected_var_ids = FxHashSet::default();

        for clause in clauses {
            let mut dominated = false;

            for (key, _) in &clause.possibilities {
                if let ClauseKey::Name(name) = key {
                    // Check if any changed var affects this clause
                    for changed_var_id in changed_var_ids {
                        let changed_name = interner.lookup(*changed_var_id);
                        if name == &*changed_name
                            || name.starts_with(&format!("{}[", changed_name))
                            || name.contains(&format!("[{}]", changed_name))
                        {
                            dominated = true;
                            affected_var_ids.insert(*changed_var_id);
                            break;
                        }
                    }
                }
                if dominated {
                    break;
                }
            }

            if !dominated {
                result.push(clause.clone());
            }
        }

        (result, affected_var_ids)
    }

    /// Merge another context into this one (for branch merging).
    pub fn merge(&mut self, other: &BlockContext) {
        // Variables assigned in both branches are definitely assigned
        let self_keys: FxHashSet<_> = self.assigned_var_ids.keys().copied().collect();
        let other_keys: FxHashSet<_> = other.assigned_var_ids.keys().copied().collect();

        let common_assigned: FxHashSet<_> = self_keys.intersection(&other_keys).copied().collect();

        // Variables assigned in either branch are possibly assigned
        let all_assigned: FxHashSet<_> = self_keys.union(&other_keys).copied().collect();

        // Update assigned_var_ids to only have common assignments
        self.assigned_var_ids
            .retain(|k, _| common_assigned.contains(k));

        // Add non-common to possibly assigned
        for var_id in all_assigned.difference(&common_assigned) {
            self.possibly_assigned_var_ids.insert(*var_id);
        }

        // Merge variable types (union of types from both branches)
        for (var_id, other_type) in &other.locals {
            if let Some(self_type) = self.locals.get(var_id) {
                // Combine types from both branches
                let combined = combine_union_types(self_type, other_type, false);
                self.locals.insert(*var_id, combined);
            } else {
                // Variable only exists in other branch - add it but mark as possibly assigned
                self.locals.insert(*var_id, other_type.clone());
                self.possibly_assigned_var_ids.insert(*var_id);
            }
        }

        let mut merged_class_string_origins = FxHashMap::default();
        for (class_var, source_var) in &self.class_string_origins {
            if other
                .class_string_origins
                .get(class_var)
                .is_some_and(|other_source| other_source == source_var)
            {
                merged_class_string_origins.insert(*class_var, *source_var);
            }
        }
        self.class_string_origins = merged_class_string_origins;

        let mut merged_aliases = FxHashMap::default();
        for (alias, target) in &self.class_aliases {
            if other
                .class_aliases
                .get(alias)
                .is_some_and(|other_target| other_target == target)
            {
                merged_aliases.insert(*alias, *target);
            }
        }
        self.class_aliases = merged_aliases;

        for (const_id, other_type) in &other.defined_constants {
            if let Some(existing_type) = self.defined_constants.get(const_id) {
                let combined = combine_union_types(existing_type, other_type, false);
                self.defined_constants.insert(*const_id, combined);
            } else {
                self.defined_constants.insert(*const_id, other_type.clone());
            }
        }

        let mut merged_constraints = self.reference_constraints.clone();
        for (var_id, constraints) in &other.reference_constraints {
            let entry = merged_constraints.entry(*var_id).or_default();
            for constraint in constraints {
                if !entry.contains(constraint) {
                    entry.push(constraint.clone());
                }
            }
        }
        self.reference_constraints = merged_constraints;

        // Keep reference bindings only when both branches agree on them.
        self.references_in_scope.retain(|ref_id, target_id| {
            other
                .references_in_scope
                .get(ref_id)
                .is_some_and(|other_target| other_target == target_id)
        });

        self.references_to_external_scope = self
            .references_to_external_scope
            .intersection(&other.references_to_external_scope)
            .copied()
            .collect();

        self.references_possibly_from_confusing_scope.extend(
            other
                .references_possibly_from_confusing_scope
                .iter()
                .copied(),
        );
    }

    /// Create/update a reference binding (`$lhs = &$rhs`).
    pub fn set_reference(
        &mut self,
        lhs_var_id: StrId,
        rhs_var_id: StrId,
        rhs_fallback_type: TUnion,
        rhs_is_external: bool,
    ) {
        self.remove_reference_binding(lhs_var_id);
        self.references_possibly_from_confusing_scope
            .remove(&lhs_var_id);

        let rhs_root = self.get_reference_root(rhs_var_id);
        let rhs_type = self
            .locals
            .get(&rhs_root)
            .cloned()
            .unwrap_or(rhs_fallback_type);

        if lhs_var_id != rhs_root {
            self.references_in_scope.insert(lhs_var_id, rhs_root);
        } else {
            self.references_in_scope.remove(&lhs_var_id);
        }

        if lhs_var_id != rhs_root
            && let Some(lhs_constraints) = self.reference_constraints.remove(&lhs_var_id)
        {
            let entry = self.reference_constraints.entry(rhs_root).or_default();
            for constraint in lhs_constraints {
                if !entry.contains(&constraint) {
                    entry.push(constraint);
                }
            }
        }

        if rhs_is_external
            || self.references_to_external_scope.contains(&rhs_var_id)
            || self.references_to_external_scope.contains(&rhs_root)
        {
            self.references_to_external_scope.insert(lhs_var_id);
        } else {
            self.references_to_external_scope.remove(&lhs_var_id);
        }

        self.propagate_reference_cluster_type(rhs_root, rhs_type);
    }

    /// Remove a variable and clean up any reference relationships.
    pub fn remove_var(&mut self, var_id: StrId) {
        self.remove_reference_binding(var_id);

        let referenced_by: Vec<StrId> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| (*target_id == var_id).then_some(*reference_id))
            .collect();

        if !referenced_by.is_empty() {
            let new_root = referenced_by[0];

            for reference_id in &referenced_by {
                self.references_in_scope.remove(reference_id);
            }

            for reference_id in referenced_by.iter().skip(1) {
                self.references_in_scope.insert(*reference_id, new_root);
            }

            if self.references_to_external_scope.remove(&var_id) {
                self.references_to_external_scope.insert(new_root);
            }

            if let Some(removed_constraints) = self.reference_constraints.remove(&var_id) {
                let entry = self.reference_constraints.entry(new_root).or_default();
                for constraint in removed_constraints {
                    if !entry.contains(&constraint) {
                        entry.push(constraint);
                    }
                }
            }

            if let Some(existing_type) = self.locals.get(&var_id).cloned() {
                self.propagate_reference_cluster_type(new_root, existing_type);
            }
        }

        self.locals.remove(&var_id);
        self.assigned_var_ids.remove(&var_id);
        self.possibly_assigned_var_ids.remove(&var_id);
        self.cond_referenced_var_ids.remove(&var_id);
        self.class_string_origins.remove(&var_id);
        self.references_to_external_scope.remove(&var_id);
        self.references_possibly_from_confusing_scope
            .remove(&var_id);
        self.reference_constraints.remove(&var_id);
    }

    /// Remove reference metadata for a single variable binding.
    pub fn remove_reference_binding(&mut self, var_id: StrId) {
        self.references_in_scope.remove(&var_id);
        self.references_to_external_scope.remove(&var_id);
    }

    pub fn mark_external_reference(&mut self, var_id: StrId) {
        self.references_to_external_scope.insert(var_id);
    }

    pub fn clear_confusing_reference(&mut self, var_id: StrId) {
        self.references_possibly_from_confusing_scope
            .remove(&var_id);
    }

    pub fn has_confusing_reference(&self, var_id: StrId) -> bool {
        self.references_possibly_from_confusing_scope
            .contains(&var_id)
    }

    /// Track references that escaped from a confusing scope (if/loop) into this scope.
    pub fn update_references_possibly_from_confusing_scope(
        &mut self,
        confusing_scope_context: &BlockContext,
    ) {
        for reference_id in confusing_scope_context
            .references_in_scope
            .keys()
            .chain(confusing_scope_context.references_to_external_scope.iter())
        {
            if !self.references_in_scope.contains_key(reference_id)
                && !self.references_to_external_scope.contains(reference_id)
            {
                self.references_possibly_from_confusing_scope
                    .insert(*reference_id);
            }
        }

        self.references_possibly_from_confusing_scope.extend(
            confusing_scope_context
                .references_possibly_from_confusing_scope
                .iter()
                .copied(),
        );
    }

    fn get_reference_root(&self, var_id: StrId) -> StrId {
        let mut current = var_id;
        let mut seen = FxHashSet::default();

        while let Some(next) = self.references_in_scope.get(&current) {
            if !seen.insert(current) {
                break;
            }

            if *next == current {
                break;
            }

            current = *next;
        }

        current
    }

    fn propagate_reference_cluster_type(&mut self, root_var_id: StrId, var_type: TUnion) {
        self.locals.insert(root_var_id, var_type.clone());
        *self.assigned_var_ids.entry(root_var_id).or_insert(0) += 1;

        let aliases: Vec<StrId> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| {
                (*target_id == root_var_id).then_some(*reference_id)
            })
            .collect();

        for alias in aliases {
            self.locals.insert(alias, var_type.clone());
            *self.assigned_var_ids.entry(alias).or_insert(0) += 1;
        }
    }

    fn propagate_reference_cluster_type_without_assignment(
        &mut self,
        root_var_id: StrId,
        var_type: TUnion,
    ) {
        self.locals.insert(root_var_id, var_type.clone());

        let aliases: Vec<StrId> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| {
                (*target_id == root_var_id).then_some(*reference_id)
            })
            .collect();

        for alias in aliases {
            self.locals.insert(alias, var_type.clone());
        }
    }
}

/// Context for analyzing a function or method.
#[derive(Clone, Debug, Default)]
pub struct FunctionContext {
    /// The function name.
    pub function_name: Option<StrId>,

    /// The expected return type.
    pub return_type: Option<TUnion>,

    /// Whether we've seen a return statement.
    pub has_returned: bool,

    /// Whether all code paths return a value.
    pub all_paths_return: bool,

    /// Template types in scope for this function.
    pub template_types: FxHashMap<StrId, TUnion>,

    /// The function's declaring class (for methods).
    pub declaring_class: Option<StrId>,

    /// Whether this is a static method.
    pub is_static: bool,
}

impl FunctionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_return_type(mut self, return_type: TUnion) -> Self {
        self.return_type = Some(return_type);
        self
    }
}
