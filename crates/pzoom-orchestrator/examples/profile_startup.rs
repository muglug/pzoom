//! Profiling harness replicating the WASM `ScannerAndAnalyzer::default()`
//! startup path: scan embedded stubs, apply the CallMap, populate.
use pzoom_orchestrator::{Populator, Scanner, apply_call_map};

const PHP_VERSION_ID: u32 = 8 * 10_000 + 5 * 100; // 8.5

fn startup() {
    let mut scanner = Scanner::new();
    scanner.scan_stubs(&rustc_hash::FxHashSet::default());
    let mut scan_result = scanner.finish();
    let shared_interner = scan_result.interner.into_shared();
    {
        let threaded = pzoom_str::ThreadedInterner::new(shared_interner.clone());
        apply_call_map(&mut scan_result.codebase, &threaded, PHP_VERSION_ID);
    }
    let mut interner = pzoom_str::unwrap_shared(shared_interner);
    let mut populator = Populator::new(&mut scan_result.codebase, &mut interner);
    populator.populate();
    std::hint::black_box(&scan_result.codebase);
}

fn main() {
    let iters: usize = std::env::var("ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let t = std::time::Instant::now();
    for _ in 0..iters {
        startup();
    }
    eprintln!("ran {iters} startup iteration(s) in {:?}", t.elapsed());
}
