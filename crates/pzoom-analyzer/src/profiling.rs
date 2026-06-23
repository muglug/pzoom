//! Lightweight profiling counters for the analyze phase, used to measure how
//! much per-file analysis is spent **re-parsing** (the scan phase already
//! parsed every file; `file_analyzer` parses again, and trait files are
//! re-parsed once per using class). The counters are thread-safe atomics
//! accumulated across the parallel analysis; cost is negligible. Call
//! [`dump`] after analysis to print the breakdown (the CLI gates this on the
//! `PZOOM_PARSE_STATS` env var).

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Instant;

/// Total worker-CPU time spent inside `FileAnalyzer::analyze` (summed across
/// threads). Includes the per-file re-parse, name resolution, and the actual
/// type analysis (which itself includes any trait re-parses).
pub static ANALYZE_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
/// Time in `parse_file_content` for the per-file re-parse (`file_analyzer`).
pub static PARSE_NS: AtomicU64 = AtomicU64::new(0);
/// Time in `resolve_names` for the per-file re-resolve (`file_analyzer`).
pub static RESOLVE_NS: AtomicU64 = AtomicU64::new(0);
/// Number of files analyzed.
pub static FILE_COUNT: AtomicU64 = AtomicU64::new(0);
/// Time re-parsing trait source files (once per using class).
pub static TRAIT_PARSE_NS: AtomicU64 = AtomicU64::new(0);
/// Time re-resolving names in re-parsed trait files.
pub static TRAIT_RESOLVE_NS: AtomicU64 = AtomicU64::new(0);
/// Number of trait re-parses (one per (trait, using-class) pair).
pub static TRAIT_PARSE_COUNT: AtomicU64 = AtomicU64::new(0);

/// Add `start.elapsed()` (ns) to `counter`.
#[inline]
pub fn record(counter: &AtomicU64, start: Instant) {
    counter.fetch_add(start.elapsed().as_nanos() as u64, Relaxed);
}

/// Records elapsed time into a counter on drop, covering every return path.
pub struct ScopeTimer {
    start: Instant,
    counter: &'static AtomicU64,
}

impl ScopeTimer {
    #[inline]
    pub fn new(counter: &'static AtomicU64) -> Self {
        Self { start: Instant::now(), counter }
    }
}

impl Drop for ScopeTimer {
    #[inline]
    fn drop(&mut self) {
        self.counter.fetch_add(self.start.elapsed().as_nanos() as u64, Relaxed);
    }
}

/// Print the analyze-phase parse breakdown to stderr.
pub fn dump() {
    let files = FILE_COUNT.load(Relaxed).max(1);
    let total = ANALYZE_TOTAL_NS.load(Relaxed).max(1);
    let parse = PARSE_NS.load(Relaxed);
    let resolve = RESOLVE_NS.load(Relaxed);
    let tparse = TRAIT_PARSE_NS.load(Relaxed);
    let tresolve = TRAIT_RESOLVE_NS.load(Relaxed);
    let tcount = TRAIT_PARSE_COUNT.load(Relaxed);

    let ms = |ns: u64| ns as f64 / 1e6;
    let us_file = |ns: u64| ns as f64 / 1e3 / files as f64;
    let pct = |ns: u64| 100.0 * ns as f64 / total as f64;

    eprintln!("[parse-stats] analyzed files: {files}");
    eprintln!("[parse-stats] analyze worker-CPU total: {:.0} ms", ms(total));
    eprintln!(
        "[parse-stats]   re-parse (file):    {:>7.0} ms  {:>6.1} us/file  {:>5.1}% of analyze CPU",
        ms(parse),
        us_file(parse),
        pct(parse)
    );
    eprintln!(
        "[parse-stats]   re-resolve (file):  {:>7.0} ms  {:>6.1} us/file  {:>5.1}%",
        ms(resolve),
        us_file(resolve),
        pct(resolve)
    );
    eprintln!(
        "[parse-stats] trait re-parses: {tcount} ({:.2}x per analyzed file)",
        tcount as f64 / files as f64
    );
    eprintln!(
        "[parse-stats]   trait re-parse:     {:>7.0} ms  {:>5.1}% of analyze CPU",
        ms(tparse),
        pct(tparse)
    );
    eprintln!(
        "[parse-stats]   trait re-resolve:   {:>7.0} ms  {:>5.1}%",
        ms(tresolve),
        pct(tresolve)
    );
    eprintln!(
        "[parse-stats] ALL parsing in analyze (file + trait): {:.1}% of analyze CPU",
        pct(parse + tparse)
    );
    eprintln!(
        "[parse-stats] ALL parse+resolve in analyze:          {:.1}% of analyze CPU",
        pct(parse + resolve + tparse + tresolve)
    );
}
