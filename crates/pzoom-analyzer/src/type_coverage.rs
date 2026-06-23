//! Lightweight, env-gated type-coverage accounting.
//!
//! When `PZOOM_TYPE_COVERAGE=1`, [`record`] tallies a file's finalized
//! per-expression types: how many distinct analyzed expressions resolved to a
//! concrete (non-`mixed`) type vs. the total. Disabled by default so normal
//! runs pay only a single atomic-bool check.

use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use pzoom_code_info::TUnion;

static TOTAL: AtomicU64 = AtomicU64::new(0);
static NON_MIXED: AtomicU64 = AtomicU64::new(0);

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| matches!(std::env::var("PZOOM_TYPE_COVERAGE").as_deref(), Ok("1") | Ok("true")))
}

/// Tally one file's finalized expression types.
pub fn record<'a>(types: impl IntoIterator<Item = &'a Rc<TUnion>>) {
    if !enabled() {
        return;
    }

    let mut total = 0u64;
    let mut non_mixed = 0u64;
    for ty in types {
        total += 1;
        if !ty.is_mixed() {
            non_mixed += 1;
        }
    }

    TOTAL.fetch_add(total, Ordering::Relaxed);
    NON_MIXED.fetch_add(non_mixed, Ordering::Relaxed);
}

/// Returns `(total_expressions, non_mixed_expressions)`.
pub fn snapshot() -> (u64, u64) {
    (TOTAL.load(Ordering::Relaxed), NON_MIXED.load(Ordering::Relaxed))
}
