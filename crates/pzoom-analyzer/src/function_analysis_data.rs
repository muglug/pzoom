//! Central state container for function/method analysis.
//!
//! Holds all accumulated data during analysis of a function body.

use pzoom_code_info::{DataFlowGraph, Issue, TUnion, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use std::rc::Rc;

use crate::scope::LoopScope;

/// Position in source code (start_offset, end_offset).
pub type Pos = (u32, u32);

/// Central state container for analyzing a function or method.
///
/// This struct accumulates all information discovered during analysis,
/// including inferred types, issues, and control flow information.
#[derive(Debug, Default)]
pub struct FunctionAnalysisData {
    /// Inferred types for expressions, keyed by source position.
    pub expr_types: FxHashMap<Pos, Rc<TUnion>>,

    /// Issues discovered during analysis.
    pub issues: Vec<Issue>,

    /// Return types inferred from return statements.
    pub inferred_return_types: Vec<TUnion>,

    /// Yield key/value types inferred from yield expressions.
    /// Key is None for `yield $value` and Some for `yield $key => $value`.
    pub inferred_yield_types: Vec<(Option<TUnion>, TUnion)>,

    /// Whether the function-like currently being analyzed is a generator (its body
    /// contains a `yield`/`yield from`). Set before analyzing each function-like body
    /// and restored afterwards for nested scopes.
    pub current_function_is_generator: bool,

    /// Whether the function definitely returns on all paths.
    pub all_paths_return: bool,

    /// Current statement's start position (for issue reporting).
    pub current_stmt_start: Option<u32>,

    /// Current statement's end position.
    pub current_stmt_end: Option<u32>,

    /// Assertions that hold when an expression is truthy.
    /// Key is expression position, value is map from variable name to narrowed type.
    pub if_true_assertions: FxHashMap<Pos, FxHashMap<StrId, TUnion>>,

    /// Assertions that hold when an expression is falsy.
    pub if_false_assertions: FxHashMap<Pos, FxHashMap<StrId, TUnion>>,

    /// Function-body data-flow graph used for parent-node tracking.
    pub data_flow_graph: DataFlowGraph,

    /// Variables that have been referenced (for unused variable detection).
    pub referenced_var_ids: FxHashMap<StrId, u32>,

    /// Variables that have been assigned (for definite assignment analysis).
    pub assigned_var_ids: FxHashMap<StrId, u32>,

    /// Class-like names declared in the current file (for duplicate declaration checks).
    pub declared_classlike_names: FxHashMap<StrId, u32>,

    /// Observed argument types at named-function callsites, keyed by (function_id, param_index).
    ///
    /// This is used for flow-aware refinement when a function body is analyzed after
    /// callsites in the same file.
    pub function_argument_callsite_types: FxHashMap<(StrId, usize), TUnion>,

    /// Effects from one expression copied to another (for control flow).
    effects: FxHashMap<Pos, Vec<Pos>>,

    /// Active loop scopes, innermost last. Pushed by the loop analyzer before
    /// analyzing a loop body and popped afterwards; `break`/`continue` update the
    /// top of this stack. Mirrors Hakana's threaded `&mut Option<LoopScope>`.
    pub loop_scopes: Vec<LoopScope>,

    /// Number of nested issue-recording sessions currently active. When > 0, newly
    /// added issues are buffered instead of emitted (used by the loop fixpoint so
    /// only the final iteration's issues are reported). Mirrors Hakana.
    recording_level: usize,

    /// Stack of buffered issues, one buffer per active recording session.
    recorded_issues: Vec<Vec<Issue>>,
}

impl FunctionAnalysisData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store the inferred type for an expression at the given position.
    pub fn set_expr_type(&mut self, pos: Pos, expr_type: TUnion) {
        self.expr_types.insert(pos, Rc::new(expr_type));
    }

    /// Get the inferred type for an expression at the given position.
    pub fn get_expr_type(&self, pos: Pos) -> Option<Rc<TUnion>> {
        self.expr_types.get(&pos).cloned()
    }

    /// Get the inferred type, or mixed if not found.
    pub fn get_expr_type_or_mixed(&self, pos: Pos) -> Rc<TUnion> {
        self.expr_types
            .get(&pos)
            .cloned()
            .unwrap_or_else(|| Rc::new(TUnion::mixed()))
    }

    /// Add an issue to be reported.
    ///
    /// While issue recording is active (inside the loop fixpoint), the issue is
    /// buffered in the current recording session instead of being emitted.
    pub fn add_issue(&mut self, issue: Issue) {
        if self.recording_level > 0 {
            if let Some(buffer) = self.recorded_issues.last_mut() {
                buffer.push(issue);
            }
            return;
        }
        self.issues.push(issue);
    }

    /// Add an issue if it's not suppressed by configuration.
    pub fn maybe_add_issue(&mut self, issue: Issue, _suppressed: &[String]) {
        // TODO: Check against suppressed issue types
        self.add_issue(issue);
    }

    /// Begin a new issue-recording session. Issues added while recording is active
    /// are buffered rather than emitted.
    pub fn start_recording_issues(&mut self) {
        self.recording_level += 1;
        self.recorded_issues.push(Vec::new());
    }

    /// End the current issue-recording session.
    pub fn stop_recording_issues(&mut self) {
        if self.recording_level > 0 {
            self.recording_level -= 1;
        }
    }

    /// Take and clear the issues buffered in the current recording session.
    pub fn clear_currently_recorded_issues(&mut self) -> Vec<Issue> {
        self.recorded_issues.pop().unwrap_or_default()
    }

    /// Re-emit a previously recorded issue. If an outer recording session is still
    /// active the issue is re-buffered there; otherwise it is emitted.
    pub fn bubble_up_issue(&mut self, issue: Issue) {
        if self.recording_level == 0 {
            self.issues.push(issue);
            return;
        }
        if let Some(buffer) = self.recorded_issues.last_mut() {
            buffer.push(issue);
        }
    }

    /// Record that a variable was referenced.
    pub fn record_var_reference(&mut self, var_id: StrId, pos: u32) {
        self.referenced_var_ids.entry(var_id).or_insert(pos);
    }

    /// Record that a variable was assigned.
    pub fn record_var_assignment(&mut self, var_id: StrId, pos: u32) {
        self.assigned_var_ids.entry(var_id).or_insert(pos);
    }

    /// Add a return type inferred from a return statement.
    pub fn add_return_type(&mut self, return_type: TUnion) {
        self.inferred_return_types.push(return_type);
    }

    /// Combine the inferred return types recorded since `start_index` into one
    /// union, returning `void` when none were recorded. Shared by the function
    /// and closure/arrow analyzers when materializing an inferred return type.
    pub fn combine_inferred_return_types(&self, start_index: usize) -> TUnion {
        let new_return_types = &self.inferred_return_types[start_index..];
        if new_return_types.is_empty() {
            return TUnion::void();
        }

        let mut combined = new_return_types[0].clone();
        for return_type in &new_return_types[1..] {
            combined = combine_union_types(&combined, return_type, false);
        }
        combined
    }

    /// Add yield key/value types inferred from a yield expression.
    pub fn add_yield_type(&mut self, key_type: Option<TUnion>, value_type: TUnion) {
        self.inferred_yield_types.push((key_type, value_type));
    }

    /// Set assertions for when an expression is truthy.
    pub fn set_if_true_assertions(&mut self, pos: Pos, assertions: FxHashMap<StrId, TUnion>) {
        self.if_true_assertions.insert(pos, assertions);
    }

    /// Set assertions for when an expression is falsy.
    pub fn set_if_false_assertions(&mut self, pos: Pos, assertions: FxHashMap<StrId, TUnion>) {
        self.if_false_assertions.insert(pos, assertions);
    }

    /// Get assertions for when an expression is truthy.
    pub fn get_if_true_assertions(&self, pos: Pos) -> Option<&FxHashMap<StrId, TUnion>> {
        self.if_true_assertions.get(&pos)
    }

    /// Get assertions for when an expression is falsy.
    pub fn get_if_false_assertions(&self, pos: Pos) -> Option<&FxHashMap<StrId, TUnion>> {
        self.if_false_assertions.get(&pos)
    }

    /// Get the expression type as an Rc.
    pub fn get_rc_expr_type(&self, pos: Pos) -> Option<&Rc<TUnion>> {
        self.expr_types.get(&pos)
    }

    /// Set the expression type with an existing Rc.
    pub fn set_rc_expr_type(&mut self, pos: Pos, expr_type: Rc<TUnion>) {
        self.expr_types.insert(pos, expr_type);
    }

    /// Copy effects from one position to another.
    ///
    /// This is used to propagate effects from sub-expressions to parent expressions.
    pub fn copy_effects(&mut self, from_pos: Pos, to_pos: Pos) {
        self.effects.entry(to_pos).or_default().push(from_pos);
    }

    /// Combine effects from two positions into a target position.
    ///
    /// This is used when analyzing branches (e.g., ternary expressions).
    pub fn combine_effects(&mut self, from_pos1: Pos, from_pos2: Pos, to_pos: Pos) {
        let mut combined = vec![from_pos1, from_pos2];

        // Also copy any effects that were recorded for the source positions
        if let Some(effects1) = self.effects.get(&from_pos1) {
            combined.extend(effects1.clone());
        }
        if let Some(effects2) = self.effects.get(&from_pos2) {
            combined.extend(effects2.clone());
        }

        self.effects.entry(to_pos).or_default().extend(combined);
    }

    /// Get effects for a position.
    pub fn get_effects(&self, pos: Pos) -> Option<&Vec<Pos>> {
        self.effects.get(&pos)
    }

    pub fn record_function_argument_callsite_type(
        &mut self,
        function_id: StrId,
        param_index: usize,
        arg_type: TUnion,
    ) {
        let key = (function_id, param_index);
        if let Some(existing) = self.function_argument_callsite_types.get_mut(&key) {
            *existing = combine_union_types(existing, &arg_type, false);
        } else {
            self.function_argument_callsite_types.insert(key, arg_type);
        }
    }

    pub fn get_function_argument_callsite_type(
        &self,
        function_id: StrId,
        param_index: usize,
    ) -> Option<&TUnion> {
        self.function_argument_callsite_types
            .get(&(function_id, param_index))
    }
}
