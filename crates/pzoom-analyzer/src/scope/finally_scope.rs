//! Port of Psalm `Internal/Scope/FinallyScope`.
//!
//! Threaded through `BlockContext` while a try/catch with a `finally` clause is
//! analyzed: every control-flow exit inside the try/catch (a `return`, and in
//! Psalm also `break`/`continue`/`throw`) merges its in-scope variables here, so
//! the finally block sees variables that were only assigned on some exit paths
//! as possibly undefined.

use pzoom_code_info::{TUnion, VarName};
use rustc_hash::FxHashMap;

/// Variables collected from the exit points of a try/catch, for analysis of the
/// associated `finally` block (Psalm's `FinallyScope::$vars_in_scope`).
#[derive(Clone, Debug, Default)]
pub struct FinallyScope {
    pub vars_in_scope: FxHashMap<VarName, TUnion>,
}
