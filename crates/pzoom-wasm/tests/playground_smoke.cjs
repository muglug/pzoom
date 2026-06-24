// Smoke test for the pzoom.dev playground engine (the pzoom-wasm crate).
//
// A `--target nodejs` build is the same `wasm32-unknown-unknown` binary the
// browser loads on pzoom.dev -- only the JS glue differs -- so actually running
// the analyzer here catches runtime traps a plain `cargo build` cannot. Native
// builds have a system clock and nothing exercises the wasm at runtime, so a
// panic that only fires on `wasm32-unknown-unknown` (e.g. `std::time::Instant`,
// which is unsupported there: `Instant::now()` panics with "time not
// implemented on this platform", surfacing in-browser as
// `RuntimeError: unreachable executed`) slips through every other CI job.
//
// Usage: node playground_smoke.cjs <pkg-dir>
//   <pkg-dir> defaults to ../pkg-node (the wasm-pack --out-dir used in CI).

'use strict';

const path = require('path');

const pkgDir = process.argv[2] || path.join(__dirname, '..', 'pkg-node');
const { ScannerAndAnalyzer } = require(path.resolve(pkgDir, 'pzoom_wasm.js'));

// Two snippets, chosen to exercise both profiling-timer call sites:
//   1. the default pzoom.dev snippet (FileAnalyzer::analyze -> per-file timers)
//   2. a trait use (class_analyzer trait re-parse -> trait timers)
const snippets = [
  `<?php
function takesAnInt(int $i): void {}
$data = ["some text", 5];
takesAnInt($data[0]);
$condition = rand(0, 5);
if ($condition) {
} elseif ($condition) {}
`,
  `<?php
trait Greet { public function hi(): string { return "hi"; } }
class C { use Greet; }
echo (new C())->hi();
`,
];

// Constructing the analyzer scans the embedded stubs + CallMap; each call below
// throws (a wasm trap becomes a JS RuntimeError) if the engine panics.
const analyzer = new ScannerAndAnalyzer();

for (let i = 0; i < snippets.length; i++) {
  const raw = analyzer.get_results(snippets[i]);
  let parsed;
  try {
    parsed = JSON.parse(raw);
  } catch (e) {
    console.error(`snippet ${i}: get_results did not return valid JSON: ${raw}`);
    process.exit(1);
  }
  if (!Array.isArray(parsed.results)) {
    console.error(`snippet ${i}: expected { "results": [...] }, got: ${raw}`);
    process.exit(1);
  }
  console.log(`snippet ${i}: ok (${parsed.results.length} issue(s))`);
}

console.log('playground smoke test passed');
