//! Analysis context - tracks scope state during analysis.
//!
//! Modeled after Psalm's Context and hakana's BlockContext.

use std::cell::RefCell;
use std::rc::Rc;

use pzoom_code_info::algebra::{Clause, ClauseKey};
use pzoom_code_info::{TAtomic, TUnion, combine_union_types};

/// Expand any `bool` atomic into the pair `true`, `false` for set arithmetic.
fn expand_bool_atomics(atomics: &[TAtomic]) -> Vec<TAtomic> {
    let mut expanded = Vec::with_capacity(atomics.len());
    for atomic in atomics {
        if matches!(atomic, TAtomic::TBool) {
            expanded.push(TAtomic::TTrue);
            expanded.push(TAtomic::TFalse);
        } else {
            expanded.push(atomic.clone());
        }
    }
    expanded
}

/// Re-collapse a `true` + `false` pair back into `bool`.
fn collapse_bool_atomics(atomics: &mut Vec<TAtomic>) {
    let has_true = atomics.iter().any(|a| matches!(a, TAtomic::TTrue));
    let has_false = atomics.iter().any(|a| matches!(a, TAtomic::TFalse));
    if has_true && has_false {
        atomics.retain(|a| !matches!(a, TAtomic::TTrue | TAtomic::TFalse));
        atomics.push(TAtomic::TBool);
    }
}

/// Whether two unions describe the same set of atomic types, ignoring ordering
/// and data-flow noise. A lightweight stand-in for Psalm's `Union::equals`, used
/// by the if/else merge to decide whether a variable was genuinely redefined
/// (rather than merely re-emitted with different data-flow nodes by the `||`/`&&`
/// analyzers).
pub(crate) fn unions_structurally_equal(left: &TUnion, right: &TUnion) -> bool {
    if left.types.len() != right.types.len() {
        return false;
    }
    // Semantically-meaningful provenance flags still count (e.g. `from_calculation`
    // marks an int that may have overflowed to float, which a later `is_float`
    // check depends on); only data-flow node noise is ignored.
    if left.from_calculation != right.from_calculation
        || left.from_docblock != right.from_docblock
        || left.possibly_undefined != right.possibly_undefined
    {
        return false;
    }
    // Order-insensitive multiset comparison using TAtomic's exact equality, so
    // genuinely different types (e.g. distinct int literals/ranges) still count as
    // a redefinition while pure ordering/flag noise does not.
    let mut matched = vec![false; right.types.len()];
    for left_atomic in &left.types {
        let mut found = false;
        for (index, right_atomic) in right.types.iter().enumerate() {
            if !matched[index] && left_atomic == right_atomic {
                matched[index] = true;
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

/// Replace `old_type`'s atomics with `new_type`'s within `existing`, mirroring
/// Psalm's `Union::substitute` as used by `Context::update`. With `new_type` as
/// `None` the old possibility is simply removed; the result never becomes empty.
fn substitute_union(existing: &TUnion, old_type: &TUnion, new_type: Option<&TUnion>) -> TUnion {
    // Expand `bool` to `true | false` so set subtraction works (pzoom stores `bool`
    // as a single atomic, whereas the substitution is in terms of `true`/`false`).
    let old_expanded = expand_bool_atomics(&old_type.types);
    let mut atomics: Vec<TAtomic> = expand_bool_atomics(&existing.types)
        .into_iter()
        .filter(|atomic| !old_expanded.iter().any(|old| old == atomic))
        .collect();

    if let Some(new_type) = new_type {
        for atomic in expand_bool_atomics(&new_type.types) {
            if !atomics.contains(&atomic) {
                atomics.push(atomic);
            }
        }
    }

    collapse_bool_atomics(&mut atomics);

    if atomics.is_empty() {
        return existing.clone();
    }

    // An empty-array member beside another array-ish member folds (Psalm
    // reaches the same end state through TypeCombiner in its branch merges):
    // `array<never, never>|list<T>` must read as `list<T>` downstream.
    let has_empty_array = atomics.iter().any(|atomic| {
        matches!(
            atomic,
            TAtomic::TArray { key_type, value_type }
                if key_type.is_nothing() && value_type.is_nothing()
        )
    });
    if has_empty_array
        && atomics.len() > 1
        && atomics.iter().all(|atomic| {
            matches!(
                atomic,
                TAtomic::TArray { .. }
                    | TAtomic::TNonEmptyArray { .. }
                    | TAtomic::TList { .. }
                    | TAtomic::TNonEmptyList { .. }
                    | TAtomic::TKeyedArray { .. }
            )
        })
    {
        atomics = pzoom_code_info::ttype::type_combiner::combine(atomics, false);
    }

    let mut result = TUnion::from_types(atomics);
    result.from_docblock = existing.from_docblock;
    result.ignore_nullable_issues = existing.ignore_nullable_issues;
    result.ignore_falsable_issues = existing.ignore_falsable_issues;
    // Psalm's Union::substitute preserves the union's dataflow — replacing
    // atomics must not sever taint paths (`if ($x !== "") { $x = null; }`
    // still echoes the original value's taint afterwards).
    result.parent_nodes = existing.parent_nodes.clone();
    if let Some(new_type) = new_type {
        if new_type.from_docblock {
            result.from_docblock = true;
        }
        for parent_node in &new_type.parent_nodes {
            if !result
                .parent_nodes
                .iter()
                .any(|existing_node| existing_node.id == parent_node.id)
            {
                result.parent_nodes.push(parent_node.clone());
            }
        }
    }
    result
}

/// The variable a dependent `get_class`/`gettype` atomic depends on, if any.
fn dependent_type_var(atomic: &TAtomic) -> Option<&VarName> {
    match atomic {
        TAtomic::TDependentGetClass { var_id, .. } | TAtomic::TDependentGetType { var_id } => {
            Some(var_id)
        }
        _ => None,
    }
}
use pzoom_code_info::VarName;
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::stmt::scope_analyzer::{BreakContext, ControlAction};

/// Context for analyzing a block of code.
///
/// Tracks variable types, assignments, and scope state.
#[derive(Clone, Debug, Default)]
pub struct BlockContext {
    /// Variable types currently in scope: `$varName` -> Type.
    pub locals: FxHashMap<VarName, TUnion>,

    /// Variables whose conflicting clauses were evicted in this scope; branch
    /// analyzers propagate the eviction to the parent context after the body
    /// (Psalm's `Context::$parent_remove_vars`).
    pub parent_remove_vars: FxHashSet<VarName>,

    /// Variables that have definitely been assigned in this scope (with count).
    pub assigned_var_ids: FxHashMap<VarName, usize>,

    /// Variables that may have been assigned (e.g., in one branch of an if).
    pub possibly_assigned_var_ids: FxHashSet<VarName>,

    /// Variables that might be in scope at this point — a superset of `locals`
    /// that also retains variables possibly defined on some incoming path.
    /// Mirrors Psalm's `Context::$vars_possibly_in_scope`; consulted when merging
    /// branch/loop scopes to decide which variables become "possibly defined"
    /// afterwards.
    pub vars_possibly_in_scope: FxHashSet<VarName>,

    /// Variables referenced in conditional contexts.
    pub cond_referenced_var_ids: FxHashSet<VarName>,

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

    /// Whether we're inside an isset() call (Psalm's Context::inside_isset,
    /// also set for empty() and the left side of `??`).
    pub inside_isset: bool,

    /// Whether we're analyzing an argument of class_exists()/interface_exists()
    /// /enum_exists()/trait_exists()/class_alias(): `X::class` existence checks
    /// are suppressed there (Psalm's Context::inside_class_exists).
    pub inside_class_exists: bool,

    /// Whether the expression being analyzed is the root of an array
    /// assignment target (`$out` in `$out[] = ...;`) — a write position, so
    /// undefined/possibly-undefined variable reads are not reported (Psalm
    /// seeds undeclared roots as fresh arrays).
    pub inside_assignment_root: bool,

    /// Whether we're inside an unset() call.
    pub inside_unset: bool,

    /// Whether we're analyzing an argument passed to a by-ref parameter.
    /// Psalm never read-analyzes such an argument when its var path isn't in
    /// scope, so an empty-array fetch there stays silent (the call writes the
    /// offset rather than reading it).
    pub inside_by_ref_argument: bool,

    /// Whether we're analyzing the value of an assignment (Psalm's
    /// `Context::inside_assignment`) — the result of a call here is "used".
    pub inside_assignment: bool,

    /// Whether we're analyzing a call argument (Psalm's `Context::inside_call`).
    pub inside_call: bool,

    /// Whether we're analyzing a returned expression (Psalm's
    /// `Context::inside_return`).
    pub inside_return: bool,

    /// Whether the property fetch being analyzed is the root of an array
    /// APPEND target (`$a->foo[] = …`) — Psalm doesn't count that as a read
    /// of the property for find_unused_code (an offset write does).
    pub inside_array_append_root: bool,

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

    /// Whether we're analyzing a loop's pre-conditions or post-expressions
    /// (the header of a `for`/`while`), as opposed to its body.
    pub inside_loop_exprs: bool,

    /// Stack of enclosing break targets (loop vs switch), innermost last. Used by
    /// `break`/`continue` to decide whether they leave a switch or a loop.
    pub break_types: Vec<BreakContext>,

    /// Control-flow actions performed directly in this context (break/continue/…).
    pub control_actions: FxHashSet<ControlAction>,

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

    /// Maps in-scope references to the variable they reference (`$b => $a` for `$b = &$a`).
    pub references_in_scope: FxHashMap<VarName, VarName>,

    /// Set of references to values outside the current local scope (array offsets/properties/globals).
    pub references_to_external_scope: FxHashSet<VarName>,

    /// References that may have originated in a confusing scope (if/loop), and are unsafe to reuse.
    pub references_possibly_from_confusing_scope: FxHashSet<VarName>,

    /// Runtime class aliases declared via `class_alias`.
    pub class_aliases: FxHashMap<StrId, StrId>,

    /// Constants defined at analysis time via `define()`.
    pub defined_constants: FxHashMap<StrId, TUnion>,

    /// Type constraints imposed by by-ref parameters on referenced variables.
    pub reference_constraints: FxHashMap<VarName, Vec<TUnion>>,

    /// Foreach key variables and the list variable whose keys they iterate
    /// (stands in for Psalm's `TIntRange::$dependent_list_key`): writing
    /// `$list[$key] = ...` with such a key keeps the list a list.
    pub list_key_dependencies: FxHashMap<VarName, VarName>,

    /// Function-local static variables declared via `static $x`.
    pub static_var_ids: FxHashSet<VarName>,

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

    /// Remove `$this` and any `$this->...`-derived locals/assignment tracking.
    /// Used when entering a `static` closure/arrow scope, where `$this` is not
    /// available.
    pub fn strip_this_assumptions(&mut self) {
        let this_related_vars: Vec<VarName> = self
            .locals
            .keys()
            .cloned()
            .filter(|var_id| var_id.as_str() == "$this" || var_id.starts_with("$this->"))
            .collect();

        for var_id in this_related_vars {
            self.locals.remove(&var_id);
            self.assigned_var_ids.remove(&var_id);
            self.possibly_assigned_var_ids.remove(&var_id);
        }
    }

    /// Remove property-path locals (`$x->y`) plus their assignment tracking,
    /// class-string origins and any clauses keyed on a property path. Closures
    /// don't inherit the outer scope's property-narrowing assumptions.
    pub fn strip_property_path_assumptions(&mut self) {
        let property_path_vars: Vec<VarName> = self
            .locals
            .keys()
            .cloned()
            .filter(|var_id| var_id.contains("->"))
            .collect();

        for var_id in property_path_vars {
            self.locals.remove(&var_id);
            self.assigned_var_ids.remove(&var_id);
            self.possibly_assigned_var_ids.remove(&var_id);
        }

        self.clauses.retain(|clause| {
            !clause
                .possibilities
                .keys()
                .any(|key| matches!(key, ClauseKey::Name(name) if name.contains("->")))
        });
    }

    /// Create a child context for a nested scope.
    pub fn child(&self) -> Self {
        Self {
            locals: self.locals.clone(),
            // Psalm clones $parent_remove_vars into branch contexts and never
            // clears it: once a var is assigned anywhere earlier, every later
            // if-statement boundary replays removeVarFromConflictingClauses
            // for it on the outer context (IfAnalyzer's parent_remove_vars
            // loop), purging stale clauses that mention it.
            parent_remove_vars: self.parent_remove_vars.clone(),
            assigned_var_ids: FxHashMap::default(),
            possibly_assigned_var_ids: FxHashSet::default(),
            vars_possibly_in_scope: self.vars_possibly_in_scope.clone(),
            cond_referenced_var_ids: FxHashSet::default(),
            clauses: self.clauses.clone(),
            reconciled_expression_clauses: Vec::new(),
            has_returned: false, // Child scope starts fresh
            inside_throw: false,
            inside_conditional: self.inside_conditional,
            inside_isset: false,
            inside_class_exists: false,
            inside_assignment_root: false,
            inside_unset: false,
            inside_by_ref_argument: false,
            inside_assignment: false,
            inside_call: false,
            inside_return: false,
            inside_array_append_root: false,
            inside_general_use: self.inside_general_use,
            check_variables: self.check_variables,
            inside_loop: self.inside_loop,
            inside_loop_exprs: self.inside_loop_exprs,
            break_types: self.break_types.clone(),
            control_actions: FxHashSet::default(),
            inside_foreach: self.inside_foreach,
            inside_try: self.inside_try,
            self_class: self.self_class,
            parent_class: self.parent_class,
            has_this: self.has_this,
            namespace: self.namespace,
            references_in_scope: self.references_in_scope.clone(),
            references_to_external_scope: self.references_to_external_scope.clone(),
            references_possibly_from_confusing_scope: self
                .references_possibly_from_confusing_scope
                .clone(),
            class_aliases: self.class_aliases.clone(),
            defined_constants: self.defined_constants.clone(),
            reference_constraints: self.reference_constraints.clone(),
            list_key_dependencies: self.list_key_dependencies.clone(),
            static_var_ids: self.static_var_ids.clone(),
            expected_callable_arg_types: self.expected_callable_arg_types.clone(),
            if_body_context: None,
            function_context: self.function_context.clone(),
        }
    }

    /// Whether the expression being analyzed is in a position that uses its
    /// value (Psalm's `Context::insideUse`).
    pub fn inside_use(&self) -> bool {
        self.inside_assignment
            || self.inside_return
            || self.inside_call
            || self.inside_general_use
            || self.inside_conditional
            || self.inside_throw
            || self.inside_isset
    }

    /// Get the type of a variable, if known.
    pub fn get_var_type(&self, var_id: &str) -> Option<&TUnion> {
        self.locals.get(var_id)
    }

    /// Set the type of a variable.
    pub fn set_var_type(&mut self, var_id: impl Into<VarName>, var_type: TUnion) {
        let root_var_id = self.get_reference_root(var_id.into());
        self.propagate_reference_cluster_type(root_var_id, var_type);
    }

    /// Update the inferred type of a variable without treating it as an assignment.
    pub fn set_var_type_for_inference(&mut self, var_id: impl Into<VarName>, var_type: TUnion) {
        let root_var_id = self.get_reference_root(var_id.into());
        self.propagate_reference_cluster_type_without_assignment(root_var_id, var_type);
    }

    /// Set the type of a variable directly, without propagating through reference bindings.
    pub fn set_var_type_direct(&mut self, var_id: impl Into<VarName>, var_type: TUnion) {
        let var_id = var_id.into();
        self.locals.insert(var_id.clone(), var_type);
        *self.assigned_var_ids.entry(var_id.clone()).or_insert(0) += 1;
        // Psalm's AssignmentAnalyzer records every assignment in
        // possibly_assigned_var_ids as well as assigned_var_ids.
        self.possibly_assigned_var_ids.insert(var_id.clone());
        self.vars_possibly_in_scope.insert(var_id);
    }

    /// Check if a variable has been definitely assigned.
    pub fn is_assigned(&self, var_id: &str) -> bool {
        self.assigned_var_ids.contains_key(var_id)
    }

    /// Check if a variable might be assigned.
    pub fn is_possibly_assigned(&self, var_id: &str) -> bool {
        self.possibly_assigned_var_ids.contains(var_id)
    }

    pub fn add_reference_constraint(&mut self, var_id: impl Into<VarName>, constraint: TUnion) {
        let root_var_id = self.get_reference_root(var_id.into());
        let entry = self.reference_constraints.entry(root_var_id).or_default();
        if !entry.contains(&constraint) {
            entry.push(constraint);
        }
    }

    pub fn get_reference_constraints(&self, var_id: impl Into<VarName>) -> Option<&Vec<TUnion>> {
        let root_var_id = self.get_reference_root(var_id.into());
        self.reference_constraints.get(&root_var_id)
    }

    /// Get variables that were redefined compared to a parent context.
    pub fn get_redefined_locals(
        &self,
        parent_locals: &FxHashMap<VarName, TUnion>,
        _check_equality: bool,
        removed_vars: &mut FxHashSet<VarName>,
    ) -> FxHashMap<VarName, TUnion> {
        let mut redefined = FxHashMap::default();

        for (var_id, var_type) in &self.locals {
            if let Some(parent_type) = parent_locals.get(var_id) {
                // Variable exists in both - check if redefined
                if var_type != parent_type {
                    redefined.insert(var_id.clone(), var_type.clone());
                }
            }
        }

        // Track variables that were removed
        for var_id in parent_locals.keys() {
            if !self.locals.contains_key(var_id) {
                removed_vars.insert(var_id.clone());
            }
        }

        redefined
    }

    /// Whether a variable is currently in scope.
    pub fn has_variable(&self, var_id: &str) -> bool {
        self.locals.contains_key(var_id)
    }

    /// Propagate the changes a block made to a set of variables back into this
    /// (parent) context. Mirrors Psalm's `Context::update`: for each variable in
    /// `vars_to_update`, the type the block narrowed it to (`end_context`, unless
    /// the block leaves) replaces the pre-block type (`start_context`) within this
    /// context's union — so a negated narrowing performed inside an `if`/`elseif`
    /// branch is reflected afterwards. `updated_vars` records which variables
    /// actually changed.
    pub fn update(
        &mut self,
        start_context: &BlockContext,
        end_context: &BlockContext,
        has_leaving_statements: bool,
        vars_to_update: &FxHashSet<VarName>,
        updated_vars: &mut FxHashSet<VarName>,
    ) {
        for (var_id, old_type) in &start_context.locals {
            // Only variables that underwent some negation are eligible.
            if !vars_to_update.contains(var_id) {
                continue;
            }

            // If we're leaving, the block's possibility is effectively deleted.
            let new_type = if !has_leaving_statements && end_context.has_variable(var_id) {
                end_context.locals.get(var_id)
            } else {
                None
            };

            let Some(existing_type) = self.locals.get(var_id).cloned() else {
                if let Some(new_type) = new_type {
                    self.locals.insert(var_id.clone(), new_type.clone());
                    updated_vars.insert(var_id.clone());
                }
                continue;
            };

            // If the type changed within the block, substitute it in — but never
            // allow ourselves to remove every atomic from a union.
            let type_changed = match new_type {
                Some(new_type) => old_type != new_type,
                None => true,
            };
            let can_substitute = new_type.is_some() || existing_type.types.len() > 1;

            if type_changed && can_substitute {
                let substituted = substitute_union(&existing_type, old_type, new_type);
                self.locals.insert(var_id.clone(), substituted);
                updated_vars.insert(var_id.clone());
            }
        }
    }

    /// Psalm's `Context::getRedefinedVars`: this context's locals whose type
    /// differs from `new_vars` (full union equality, as `Union::equals`), plus
    /// — with `include_new_vars` — locals absent from `new_vars` entirely. The
    /// returned types are *this* context's.
    pub fn get_redefined_vars(
        &self,
        new_vars: &FxHashMap<VarName, TUnion>,
        include_new_vars: bool,
    ) -> FxHashMap<VarName, TUnion> {
        let mut redefined_vars = FxHashMap::default();

        for (var_id, this_type) in &self.locals {
            match new_vars.get(var_id) {
                None => {
                    if include_new_vars {
                        redefined_vars.insert(var_id.clone(), this_type.clone());
                    }
                }
                Some(new_type) => {
                    if this_type != new_type {
                        redefined_vars.insert(var_id.clone(), this_type.clone());
                    }
                }
            }
        }

        redefined_vars
    }

    /// Variables that are new or whose type/assignment-count changed between two
    /// contexts. Mirrors Hakana's `BlockContext::get_new_or_updated_locals`.
    pub fn get_new_or_updated_locals(original: &Self, new: &Self) -> FxHashSet<VarName> {
        let mut redefined_var_ids = FxHashSet::default();

        for (var_id, new_type) in &new.locals {
            if let Some(original_type) = original.locals.get(var_id) {
                if original.assigned_var_ids.get(var_id).copied().unwrap_or(0)
                    != new.assigned_var_ids.get(var_id).copied().unwrap_or(0)
                    || original_type != new_type
                {
                    redefined_var_ids.insert(var_id.clone());
                }
            } else {
                redefined_var_ids.insert(var_id.clone());
            }
        }

        redefined_var_ids
    }

    /// Invalidate any local whose type is a *dependent* `get_class($var_id)` /
    /// `gettype($var_id)` result, because `$var_id`'s value has just changed.
    /// Mirrors Psalm's `DependentType::getReplacement()`: the remembered
    /// dependency no longer holds, so the type collapses to its plain equivalent
    /// (`class-string` / `string`). Without this, `$t = get_class($a); $a = new
    /// B(); switch ($t)` would wrongly narrow the *new* `$a`.
    pub fn invalidate_dependent_types(&mut self, var_id: &str) {
        for local_type in self.locals.values_mut() {
            if !local_type
                .types
                .iter()
                .any(|atomic| dependent_type_var(atomic).is_some_and(|v| v == var_id))
            {
                continue;
            }
            let replaced: Vec<TAtomic> = local_type
                .types
                .iter()
                .map(|atomic| match atomic {
                    TAtomic::TDependentGetClass { var_id: v, .. } if *v == *var_id => {
                        TAtomic::TClassString { as_type: None }
                    }
                    TAtomic::TDependentGetType { var_id: v } if *v == *var_id => TAtomic::TString,
                    other => other.clone(),
                })
                .collect();
            *local_type = TUnion::from_types(replaced);
        }
    }

    /// Drop any clauses that mention `var_id`, because its type has changed.
    /// Mirrors Hakana's `remove_var_from_conflicting_clauses` (simplified: pzoom
    /// always discards conflicting clauses rather than reconciling them).
    pub fn remove_var_from_conflicting_clauses(&mut self, var_id: impl Into<VarName>) {
        let var_id = var_id.into();
        let mut changed = FxHashSet::default();
        changed.insert(var_id.clone());
        self.clauses = BlockContext::remove_reconciled_clause_refs(&self.clauses, &changed).0;
        self.parent_remove_vars.insert(var_id);
    }

    /// Drops any clauses mentioning `var_name` (or a path rooted in it) and
    /// records the eviction for parent propagation — Psalm's
    /// `Context::removeVarFromConflictingClauses`, string-keyed.
    pub fn remove_var_name_from_conflicting_clauses(&mut self, var_name: &str) {
        self.remove_var_name_clauses(var_name);
        self.parent_remove_vars.insert(VarName::new(var_name));
    }

    /// Clause removal without the parent_remove_vars marking: Psalm only
    /// reaches removeVarFromConflictingClauses on assignment when the var
    /// already existed in scope (removeDescendents is gated on
    /// `isset($context->vars_in_scope[$var_id])`), so a first assignment
    /// must not seed the if-boundary replay.
    pub fn remove_var_name_clauses(&mut self, var_name: &str) {
        use pzoom_code_info::algebra::ClauseKey;

        self.clauses.retain(|clause| {
            !clause.possibilities.keys().any(|key| match key {
                ClauseKey::Name(name) => {
                    name == var_name
                        || name.starts_with(&format!("{}[", var_name))
                        || name.starts_with(&format!("{}->", var_name))
                        || name.contains(&format!("[{}]", var_name))
                }
                ClauseKey::Range(..) => false,
            })
        });
    }

    /// Remove reconciled clause refs from a set of clauses.
    ///
    /// Returns the filtered clauses and a set of changed variable IDs.
    pub fn remove_reconciled_clause_refs(
        clauses: &[Rc<Clause>],
        changed_var_ids: &FxHashSet<VarName>,
    ) -> (Vec<Rc<Clause>>, FxHashSet<VarName>) {
        use pzoom_code_info::algebra::ClauseKey;

        let mut result = Vec::new();
        let mut affected_var_ids = FxHashSet::default();

        for clause in clauses {
            let mut dominated = false;

            for key in clause.possibilities.keys() {
                if let ClauseKey::Name(name) = key {
                    // Check if any changed var affects this clause
                    for changed_name in changed_var_ids {
                        if name == changed_name
                            || name.starts_with(&format!("{}[", changed_name))
                            || name.contains(&format!("[{}]", changed_name))
                        {
                            dominated = true;
                            affected_var_ids.insert(changed_name.clone());
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

    /// Partition clauses by a set of changed variables, mirroring Hakana's
    /// `BlockContext::remove_reconciled_clause_refs` which returns `(kept, removed)`.
    ///
    /// A clause is *removed* (reconciled) when any of its possibilities references a
    /// changed variable (or an array offset rooted at one). The removed clauses are
    /// what an `&&`/`||`/ternary records into `reconciled_expression_clauses` so the
    /// enclosing if/ternary body reconcile does not re-report them.
    pub fn partition_reconciled_clause_refs(
        clauses: &[Rc<Clause>],
        changed_var_ids: &FxHashSet<VarName>,
    ) -> (Vec<Rc<Clause>>, Vec<Rc<Clause>>) {
        use pzoom_code_info::algebra::ClauseKey;

        let mut kept = Vec::new();
        let mut removed = Vec::new();

        for clause in clauses {
            let mut dominated = false;

            for (key, _) in clause.possibilities.iter() {
                if let ClauseKey::Name(name) = key {
                    for changed_name in changed_var_ids {
                        if name == changed_name
                            || name.starts_with(&format!("{}[", changed_name))
                            || name.contains(&format!("[{}]", changed_name))
                        {
                            dominated = true;
                            break;
                        }
                    }
                }
                if dominated {
                    break;
                }
            }

            if dominated {
                removed.push(clause.clone());
            } else {
                kept.push(clause.clone());
            }
        }

        (kept, removed)
    }

    /// Merge another context into this one (for branch merging).
    pub fn merge(&mut self, other: &BlockContext) {
        // Variables assigned in both branches are definitely assigned
        let self_keys: FxHashSet<_> = self.assigned_var_ids.keys().cloned().collect();
        let other_keys: FxHashSet<_> = other.assigned_var_ids.keys().cloned().collect();

        let common_assigned: FxHashSet<_> = self_keys.intersection(&other_keys).cloned().collect();

        // Variables assigned in either branch are possibly assigned
        let all_assigned: FxHashSet<_> = self_keys.union(&other_keys).cloned().collect();

        // Update assigned_var_ids to only have common assignments
        self.assigned_var_ids
            .retain(|k, _| common_assigned.contains(k));

        // Add non-common to possibly assigned
        for var_id in all_assigned.difference(&common_assigned) {
            self.possibly_assigned_var_ids.insert(var_id.clone());
        }

        // Anything possibly in scope in either branch is possibly in scope after.
        self.vars_possibly_in_scope
            .extend(other.vars_possibly_in_scope.iter().cloned());

        // Merge variable types (union of types from both branches)
        for (var_id, other_type) in &other.locals {
            if let Some(self_type) = self.locals.get(var_id) {
                // Combine types from both branches
                let combined = combine_union_types(self_type, other_type, false);
                self.locals.insert(var_id.clone(), combined);
            } else {
                // Variable only exists in other branch - add it but mark as possibly assigned
                self.locals.insert(var_id.clone(), other_type.clone());
                self.possibly_assigned_var_ids.insert(var_id.clone());
            }
        }

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
            let entry = merged_constraints.entry(var_id.clone()).or_default();
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
            .cloned()
            .collect();

        self.references_possibly_from_confusing_scope.extend(
            other
                .references_possibly_from_confusing_scope
                .iter()
                .cloned(),
        );
    }

    /// Create/update a reference binding (`$lhs = &$rhs`).
    pub fn set_reference(
        &mut self,
        lhs_var_id: impl Into<VarName>,
        rhs_var_id: impl Into<VarName>,
        rhs_fallback_type: TUnion,
        rhs_is_external: bool,
    ) {
        let lhs_var_id = lhs_var_id.into();
        let rhs_var_id = rhs_var_id.into();
        self.remove_reference_binding(&lhs_var_id);
        self.references_possibly_from_confusing_scope
            .remove(&lhs_var_id);

        let rhs_root = self.get_reference_root(rhs_var_id.clone());
        let rhs_type = self
            .locals
            .get(&rhs_root)
            .cloned()
            .unwrap_or(rhs_fallback_type);

        if lhs_var_id != rhs_root {
            self.references_in_scope
                .insert(lhs_var_id.clone(), rhs_root.clone());
        } else {
            self.references_in_scope.remove(&lhs_var_id);
        }

        if lhs_var_id != rhs_root
            && let Some(lhs_constraints) = self.reference_constraints.remove(&lhs_var_id)
        {
            let entry = self
                .reference_constraints
                .entry(rhs_root.clone())
                .or_default();
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
    pub fn remove_var(&mut self, var_id: &str) {
        self.remove_reference_binding(var_id);

        let referenced_by: Vec<VarName> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| {
                (target_id == var_id).then(|| reference_id.clone())
            })
            .collect();

        if !referenced_by.is_empty() {
            let new_root = referenced_by[0].clone();

            for reference_id in &referenced_by {
                self.references_in_scope.remove(reference_id);
            }

            for reference_id in referenced_by.iter().skip(1) {
                self.references_in_scope
                    .insert(reference_id.clone(), new_root.clone());
            }

            if self.references_to_external_scope.remove(var_id) {
                self.references_to_external_scope.insert(new_root.clone());
            }

            if let Some(removed_constraints) = self.reference_constraints.remove(var_id) {
                let entry = self
                    .reference_constraints
                    .entry(new_root.clone())
                    .or_default();
                for constraint in removed_constraints {
                    if !entry.contains(&constraint) {
                        entry.push(constraint);
                    }
                }
            }

            if let Some(existing_type) = self.locals.get(var_id).cloned() {
                self.propagate_reference_cluster_type(new_root, existing_type);
            }
        }

        self.locals.remove(var_id);
        self.assigned_var_ids.remove(var_id);
        self.possibly_assigned_var_ids.remove(var_id);
        self.cond_referenced_var_ids.remove(var_id);
        self.references_to_external_scope.remove(var_id);
        self.references_possibly_from_confusing_scope.remove(var_id);
        self.reference_constraints.remove(var_id);
    }

    /// Remove reference metadata for a single variable binding.
    pub fn remove_reference_binding(&mut self, var_id: &str) {
        self.references_in_scope.remove(var_id);
        self.references_to_external_scope.remove(var_id);
    }

    pub fn mark_external_reference(&mut self, var_id: impl Into<VarName>) {
        self.references_to_external_scope.insert(var_id.into());
    }

    pub fn clear_confusing_reference(&mut self, var_id: &str) {
        self.references_possibly_from_confusing_scope.remove(var_id);
    }

    pub fn has_confusing_reference(&self, var_id: &str) -> bool {
        self.references_possibly_from_confusing_scope
            .contains(var_id)
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
                    .insert(reference_id.clone());
            }
        }

        self.references_possibly_from_confusing_scope.extend(
            confusing_scope_context
                .references_possibly_from_confusing_scope
                .iter()
                .cloned(),
        );
    }

    fn get_reference_root(&self, var_id: VarName) -> VarName {
        let mut current = var_id;
        let mut seen = FxHashSet::default();

        while let Some(next) = self.references_in_scope.get(&current) {
            if !seen.insert(current.clone()) {
                break;
            }

            if *next == current {
                break;
            }

            current = next.clone();
        }

        current
    }

    fn propagate_reference_cluster_type(&mut self, root_var_id: VarName, var_type: TUnion) {
        self.locals.insert(root_var_id.clone(), var_type.clone());
        *self
            .assigned_var_ids
            .entry(root_var_id.clone())
            .or_insert(0) += 1;
        // Psalm's AssignmentAnalyzer records every assignment in
        // possibly_assigned_var_ids as well as assigned_var_ids.
        self.possibly_assigned_var_ids.insert(root_var_id.clone());
        self.vars_possibly_in_scope.insert(root_var_id.clone());

        let aliases: Vec<VarName> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| {
                (*target_id == root_var_id).then(|| reference_id.clone())
            })
            .collect();

        for alias in aliases {
            // The alias keeps its own dataflow history (notably its
            // reference-binding node) in addition to the new write's: reading
            // the alias later uses both the write and the binding.
            let mut alias_type = var_type.clone();
            if let Some(existing) = self.locals.get(&alias) {
                for parent_node in &existing.parent_nodes {
                    if !alias_type.parent_nodes.contains(parent_node) {
                        alias_type.parent_nodes.push(parent_node.clone());
                    }
                }
            }
            self.locals.insert(alias.clone(), alias_type);
            *self.assigned_var_ids.entry(alias.clone()).or_insert(0) += 1;
            self.possibly_assigned_var_ids.insert(alias.clone());
            self.vars_possibly_in_scope.insert(alias);
        }
    }

    fn propagate_reference_cluster_type_without_assignment(
        &mut self,
        root_var_id: VarName,
        var_type: TUnion,
    ) {
        self.locals.insert(root_var_id.clone(), var_type.clone());

        let aliases: Vec<VarName> = self
            .references_in_scope
            .iter()
            .filter_map(|(reference_id, target_id)| {
                (*target_id == root_var_id).then(|| reference_id.clone())
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
