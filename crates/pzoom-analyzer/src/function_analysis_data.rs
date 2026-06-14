//! Central state container for function/method analysis.
//!
//! Holds all accumulated data during analysis of a function body.

use pzoom_code_info::{DataFlowGraph, Issue, TUnion, VarName, combine_union_types};
use pzoom_str::StrId;
use rustc_hash::{FxHashMap, FxHashSet};
use std::rc::Rc;

use crate::scope::LoopScope;

/// Position in source code (start_offset, end_offset).
pub type Pos = (u32, u32);

/// Where a parameter's variable-use source node came from, so unused-param
/// reporting can group per function-like and apply Psalm's trailing rule.
#[derive(Debug, Clone)]
pub struct ParamSourceInfo {
    pub node_id: pzoom_code_info::data_flow::node::DataFlowNodeId,
    /// Start offset of the enclosing function-like (grouping key).
    pub function_key: u32,
    pub param_index: usize,
    pub is_closure: bool,
    /// Psalm only reports params of plain functions, closures and private
    /// methods; public/protected method params are find-unused-code territory.
    pub reportable: bool,
    pub is_promoted: bool,
    /// By-ref params report only when the body neither reads nor writes them
    /// (a written one is an out-param; Psalm's unusedPassByReference vs
    /// passedByRefSimpleDefinedBefore distinction).
    pub by_ref: bool,
    /// End offset of the enclosing function-like, bounding the write scan.
    pub function_end: u32,
    pub name: String,
    pub span: (u32, u32),
    /// For method params: (method_or_class_final, in_interface, has_overrides)
    /// drives PossiblyUnusedParam/UnusedParam under find_unused_code
    /// (Psalm's checkMethodParamReferences). None for functions/closures.
    pub method_param_meta: Option<(bool, bool, bool)>,
}

/// Central state container for analyzing a function or method.
///
/// This struct accumulates all information discovered during analysis,
/// including inferred types, issues, and control flow information.
#[derive(Debug, Default)]
pub struct FunctionAnalysisData {
    /// Variables to narrow to their gatekeeping param's signature type once
    /// the current call's argument verification finishes (Psalm's
    /// coerceValueAfterGatekeeperArgument; the verification chain holds the
    /// context immutably).
    pub pending_gatekeeper_coercions: Vec<(VarName, TUnion)>,
    /// Inferred types for expressions, keyed by source position.
    pub expr_types: FxHashMap<Pos, Rc<TUnion>>,

    /// Start offsets of no-arg method calls whose result was memoized
    /// (Psalm's `memoizable` node attribute from MethodCallPurityAnalyzer) —
    /// only these get assertion-finder var keys like `$e->getPrevious()`.
    pub memoizable_method_call_offsets: FxHashSet<u32>,

    /// Bounds accumulated for type variables (Hakana's
    /// `type_variable_bounds`): constraints recorded while
    /// `TAtomic::TTypeVariable` placeholders flow through the body, reconciled
    /// against each other at the end of the function.
    pub type_variable_bounds:
        FxHashMap<String, pzoom_code_info::ttype::template::TypeVariableBounds>,

    /// Issues discovered during analysis.
    pub issues: Vec<Issue>,

    /// `(property, offset)` reads of `$this->prop` reached in a constructor body
    /// while collecting initialisations, before the property was initialised
    /// (Psalm's `InstancePropertyFetchAnalyzer` UninitializedProperty check).
    /// Drained by `check_property_initialization`; populated only during the
    /// `collect_initializations` re-analysis of the constructor.
    pub collected_uninitialized_reads: Vec<(StrId, u32)>,

    /// Property-fetch expressions whose lookup failed on a known class
    /// (undefined property). Psalm's handleNonExistentProperty leaves the
    /// node untyped, so a chained fetch on it stays silent; pzoom records
    /// `mixed` plus this marker.
    pub failed_property_fetch_positions: rustc_hash::FxHashSet<Pos>,

    /// Source offsets of `@psalm-suppress` tokens that suppressed an issue at
    /// an emission-decision site (Psalm's `IssueBuffer::$used_suppressions`).
    /// The file analyzer adds filter-pass matches and reports the unmatched
    /// candidates as UnusedPsalmSuppress.
    pub used_suppression_offsets: Vec<u32>,
    /// Spans covered by a statement-level `@psalm-suppress` docblock: Psalm's
    /// StatementsAnalyzer adds the docblock's suppressions for the duration of
    /// that statement's analysis (nested statements included), so the
    /// suppression applies to the whole statement span. Entries are
    /// (docblock_start, docblock_end, stmt_start, stmt_end).
    pub stmt_suppression_ranges: Vec<(u32, u32, u32, u32)>,

    /// Spans of foreach **value** target variables (Psalm's
    /// `StatementsAnalyzer::$foreach_var_locations`): an unused assignment at
    /// one of these spans reports UnusedForeachValue instead of UnusedVariable.
    pub foreach_var_positions: Vec<(u32, u32)>,

    /// Parameter source-node metadata for Psalm's `checkParamReferences`
    /// (UnusedParam/UnusedClosureParam with the trailing-params-only rule).
    pub param_sources: Vec<ParamSourceInfo>,

    /// Classes referenced from outside themselves (new/static-call/extends/
    /// implements/signature types) — Psalm's isClassReferenced, for
    /// UnusedClass under find_unused_code.
    pub referenced_classes: rustc_hash::FxHashSet<pzoom_str::StrId>,
    /// (class, lowercase method) pairs referenced by calls (excluding
    /// self-recursion) — Psalm's isClassMethodReferenced.
    pub referenced_class_members: rustc_hash::FxHashSet<(pzoom_str::StrId, pzoom_str::StrId)>,
    /// (class, property) pairs READ somewhere — Psalm's isPropertyReferenced
    /// (writes don't count as uses).
    pub referenced_properties: rustc_hash::FxHashSet<(pzoom_str::StrId, pzoom_str::StrId)>,
    /// (class, lowercase method) pairs whose call RESULT was used — Psalm's
    /// isMethodReturnReferenced.
    pub method_returns_used: rustc_hash::FxHashSet<(pzoom_str::StrId, pzoom_str::StrId)>,

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

    /// The condition expression currently being reconciled, when narrower
    /// than the statement — Psalm's reconciler issues point at the condition
    /// (`if ($x === "a")` highlights `$x === "a"`), not the whole statement.
    pub current_reconcile_pos: Option<(u32, u32)>,

    /// Assertions that hold when an expression is truthy.
    /// Key is expression position, value is map from variable name to narrowed type.
    pub if_true_assertions: FxHashMap<Pos, FxHashMap<StrId, TUnion>>,

    /// Assertions that hold when an expression is falsy.
    pub if_false_assertions: FxHashMap<Pos, FxHashMap<StrId, TUnion>>,

    /// Function-body data-flow graph used for parent-node tracking.
    pub data_flow_graph: DataFlowGraph,

    /// Variables that have been assigned (for definite assignment analysis).
    pub assigned_var_ids: FxHashMap<StrId, u32>,

    /// Class-like names declared in the current file (for duplicate declaration checks).
    pub declared_classlike_names: FxHashMap<StrId, u32>,

    /// Observed argument types at named-function callsites, keyed by (function_id, param_index).
    ///
    /// This is used for flow-aware refinement when a function body is analyzed after
    /// callsites in the same file.
    pub function_argument_callsite_types: FxHashMap<(StrId, usize), TUnion>,

    /// Method signatures of anonymous classes analyzed in this scope, keyed by
    /// the synthetic `@anonymous-class:{file}:{offset}` name. Anonymous classes
    /// are not registered in the codebase, so method calls on them resolve
    /// through this side table instead.
    pub anonymous_class_methods:
        FxHashMap<StrId, FxHashMap<StrId, pzoom_code_info::FunctionLikeInfo>>,

    /// Top-level variable types known when a function-like declaration is
    /// analyzed. `global $x` statements clone from here, mirroring Psalm's
    /// `$global_context->vars_in_scope` lookup in GlobalAnalyzer.
    pub file_global_types: FxHashMap<pzoom_code_info::VarName, TUnion>,

    /// Function-likes (keyed by start offset) whose bodies call
    /// func_get_args(): every parameter is implicitly read (Psalm skips
    /// unused-param reporting for them).
    pub func_get_args_functions: rustc_hash::FxHashSet<u32>,

    /// Per-switch frames collecting the contexts captured at `break`
    /// statements that leave the switch (Hakana's `case_scope.break_vars`):
    /// they join the post-switch merge so a `$a = 5; break;` inside a case
    /// keeps its dataflow alive past the switch.
    pub switch_break_contexts: Vec<Vec<crate::context::BlockContext>>,

    /// Active loop scopes, innermost last. Pushed by the loop analyzer before
    /// analyzing a loop body and popped afterwards; `break`/`continue` update the
    /// top of this stack. Mirrors Hakana's threaded `&mut Option<LoopScope>`.
    pub loop_scopes: Vec<LoopScope>,

    /// Number of nested issue-recording sessions currently active. When > 0, newly
    /// added issues are buffered instead of emitted (used by the loop fixpoint so
    /// only the final iteration's issues are reported). Mirrors Hakana.
    recording_level: usize,

    /// Stack of buffered issues, one buffer per active recording session.
    pub(crate) recorded_issues: Vec<Vec<Issue>>,

    /// Vars whose current reconcile found an assertion redundant (Psalm's
    /// `$failed_reconciliation = RECONCILIATION_REDUNDANT`): reconcile_keyed_
    /// types folds these into `changed_var_ids` so the redundant fact's
    /// clauses are dropped afterwards. Scoped per reconcile call (take/
    /// restore), filled by `trigger_issue_for_impossible`.
    pub redundant_reconciled_vars: rustc_hash::FxHashSet<VarName>,

    /// Each variable's first assignment location in the current function-like
    /// (Psalm's `StatementsAnalyzer::$all_vars` / `getFirstAppearance`). Used
    /// to retract a MixedAssignment when a later always-exiting guard proves
    /// the variable non-mixed (Psalm's `IssueBuffer::remove` callers). The
    /// function-like analyzers save/clear/restore this around each body, the
    /// same scoping Psalm gets from one StatementsAnalyzer per function-like.
    pub(crate) first_var_appearances: FxHashMap<VarName, u32>,
}

impl FunctionAnalysisData {
    pub fn new() -> Self {
        Self::default()
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
        // Psalm's IssueBuffer emitted-key: issue type + file:line:column +
        // (dupe_key ?? message). Same-kind issues at the same position with
        // an equal dupe key collapse to one, even with different messages —
        // the reconciler's "Docblock-defined type int for $x is never null"
        // dedupes against the assertion finder's "int does not contain null".
        let issue_dedupe_text = issue.dupe_key.as_ref().unwrap_or(&issue.message);
        if self.issues.iter().any(|existing| {
            existing.kind == issue.kind
                && existing.location.file_path == issue.location.file_path
                && existing.location.start_line == issue.location.start_line
                && existing.location.start_column == issue.location.start_column
                && existing.dupe_key.as_ref().unwrap_or(&existing.message) == issue_dedupe_text
        }) {
            return;
        }
        self.issues.push(issue);
    }

    /// Where new issues will land right now: (emitted count, active
    /// recording-frame count). Used to delimit an emission window that must
    /// be swept afterwards (inferred-purity probes).
    pub fn issue_emission_marks(&self) -> (usize, usize) {
        (
            self.issues.len(),
            self.recorded_issues.last().map_or(0, |frame| frame.len()),
        )
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
    /// active the issue is re-buffered there; otherwise it is emitted (through
    /// the same emitted-key dedupe as a fresh emission — Psalm's IssueBuffer
    /// applies alreadyEmitted to bubbled-up recorded issues too).
    pub fn bubble_up_issue(&mut self, issue: Issue) {
        if self.recording_level == 0 {
            self.add_issue(issue);
            return;
        }
        if let Some(buffer) = self.recorded_issues.last_mut() {
            buffer.push(issue);
        }
    }

    /// Psalm's `IssueBuffer::remove`: retract an already-reported issue of
    /// `kind` whose span starts at `offset` — from the active recording frame
    /// (loop fixpoint) and from the emitted set.
    pub fn remove_issue(&mut self, kind: pzoom_code_info::IssueKind, offset: u32) {
        if let Some(frame) = self.recorded_issues.last_mut() {
            frame.retain(|issue| {
                issue.kind != kind || issue.location.start_offset != offset
            });
        }
        self.issues.retain(|issue| {
            issue.kind != kind || issue.location.start_offset != offset
        });
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

}
