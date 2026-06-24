//! Psalm-faithful type-coverage accounting.
//!
//! Replicates Psalm's `Internal\Codebase\Analyzer` mixed/non-mixed counting:
//! specific expression-analysis sites call `incrementMixedCount` /
//! `incrementNonMixedCount` (under the `!collect_initializations`, file===root,
//! non-trait guard), and the reported figure is
//! `non_mixed / (mixed + non_mixed)` — "Psalm was able to infer types for X%".
//!
//! Per-file tallies live on [`crate::function_analysis_data::FunctionAnalysisData`];
//! each file folds its tally into these globals at end of analysis. Env-gated
//! (`PZOOM_TYPE_COVERAGE=1`) so normal runs do no counting.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

static MIXED: AtomicU64 = AtomicU64::new(0);
static NON_MIXED: AtomicU64 = AtomicU64::new(0);

#[inline]
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("PZOOM_TYPE_COVERAGE").as_deref(),
            Ok("1") | Ok("true")
        )
    })
}

/// Fold one file's `[mixed, non_mixed]` tally into the global totals.
pub fn add(mixed: u32, non_mixed: u32) {
    if !enabled() {
        return;
    }
    MIXED.fetch_add(mixed as u64, Ordering::Relaxed);
    NON_MIXED.fetch_add(non_mixed as u64, Ordering::Relaxed);
}

/// Returns `(mixed, non_mixed)` global totals.
pub fn snapshot() -> (u64, u64) {
    (
        MIXED.load(Ordering::Relaxed),
        NON_MIXED.load(Ordering::Relaxed),
    )
}
