//! Central state container for function/method analysis.
//!
//! Holds all accumulated data during analysis of a function body.

use pzoom_code_info::{Issue, TUnion};
use pzoom_str::StrId;
use rustc_hash::FxHashMap;
use std::rc::Rc;

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

    /// Variables that have been referenced (for unused variable detection).
    pub referenced_var_ids: FxHashMap<StrId, u32>,

    /// Variables that have been assigned (for definite assignment analysis).
    pub assigned_var_ids: FxHashMap<StrId, u32>,

    /// Effects from one expression copied to another (for control flow).
    effects: FxHashMap<Pos, Vec<Pos>>,
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
    pub fn add_issue(&mut self, issue: Issue) {
        self.issues.push(issue);
    }

    /// Add an issue if it's not suppressed by configuration.
    pub fn maybe_add_issue(&mut self, issue: Issue, _suppressed: &[String]) {
        // TODO: Check against suppressed issue types
        self.issues.push(issue);
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
        self.effects
            .entry(to_pos)
            .or_default()
            .push(from_pos);
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

        self.effects
            .entry(to_pos)
            .or_default()
            .extend(combined);
    }

    /// Get effects for a position.
    pub fn get_effects(&self, pos: Pos) -> Option<&Vec<Pos>> {
        self.effects.get(&pos)
    }
}
