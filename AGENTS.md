# AGENTS.md

This is the canonical agent instructions file for this repository.

## Start Here

1. Read this file first — it is the canonical agent guide and includes the
   Psalm/Hakana parity workflow (see "Parity Workflow" below).
2. Reference the upstream projects pzoom is ported from:
   - Psalm: https://github.com/vimeo/psalm (PascalCase PHP, sources under `src/Psalm/`)
   - Hakana: https://github.com/slackhq/hakana (snake_case Rust, sources under `src/`)

   If these are cloned locally (e.g. alongside this repo, or at the paths in
   `PSALM_HAKANA_MAPPING.md`), prefer the local checkout to avoid web lookups.

## Fast Rules

- Use `rg`/`rg --files` for search.
- Keep behavior aligned with Psalm first, then Hakana where Psalm is unclear.
- For expected test output changes, update `output.txt` only after confirming behavior parity.
- Keep error file paths relative in emitted issues (not absolute).

## Build Commands

```bash
cargo build                      # Build all crates
cargo test                       # Run unit tests
cargo test -p pzoom-analyzer     # Run tests for a specific crate
cargo run -- analyze path/to/php # Run the analyzer
```

## Running Inference Tests

**Always use `--release` and `--reuse-codebase` when running all tests:**

```bash
# Fast: Run all inference tests (recommended)
cargo run --release -p pzoom-test-runner -- --reuse-codebase

# Run specific test category
cargo run --release -p pzoom-test-runner -- --reuse-codebase tests/inference/Assignment

# Update expected output files
cargo run --release -p pzoom-test-runner -- --reuse-codebase --update
```

The `--reuse-codebase` flag scans stubs once and reuses them across all tests (~5x faster).
The `--release` flag enables compiler optimizations (much faster for large test suites).

## Measuring Psalm Parity

pzoom ships scripts that score how faithfully each file mirrors the Psalm code it
ports, to guide what to port next and to gate against drift. `psalm_parity.py` is
the method-level scorer (run it against `.parity-baseline.json` to check a
change), `similarity_heuristic.py` does broader pzoom↔Psalm/Hakana file matching,
and `parity_trend.py` charts parity over git history. The scores are a
**relative** signal — never "improve" them by renaming Rust symbols to match
Psalm tokens; that games the metric without changing behavior.

**For anything about parity — how the scores work, the baseline/CI gate,
generated reports, or using the backlog to decide what to port — see
`scripts/README.md`.**

## Parity Workflow

Goals: match Psalm's issues and type reasoning; use Hakana as an
implementation reference when it already solved a similar problem; avoid
regressions in inference test expectations.

1. Reproduce and isolate.
   - Run a focused suite:
     `cargo run --release -p pzoom-test-runner -- --reuse-codebase tests/inference/<Suite>`
   - If needed, run pzoom against a real codebase:
     `cargo run --release -p pzoom-cli -- analyze ../psalm`
2. Compare behavior. Find the equivalent logic in Psalm first, then the
   matching/ported logic in Hakana when available.
3. Re-run focused checks: `cargo check` and the focused inference suite(s).
4. Re-run the full inference suite before completion:
   `cargo run --release -p pzoom-test-runner -- --reuse-codebase`

### Diffing tips

- Compare by symbol name with `rg` in both repos.
- Grep for issue kinds to see where they are actually emitted.
- Grep for method/property resolution fallbacks in analyzer code paths.

### Definition of done

- `cargo check` passes.
- Relevant focused suites pass.
- The full inference run is clean, or every remaining output difference is
  explained and intentional.
- Changes are aligned with Psalm behavior (and Hakana where applicable).

## Architecture

pzoom is a PHP static analyzer written in Rust. It uses a three-phase analysis pipeline:

1. **Scan** (`pzoom-orchestrator/scanner.rs`) - Parse PHP files using mago parser, collect symbols (classes, functions, etc.)
2. **Populate** (`pzoom-orchestrator/populator.rs`) - Resolve inheritance hierarchies and build complete type information
3. **Analyze** (`pzoom-orchestrator/analyzer.rs`) - Type check and detect issues

### Crate Dependencies

```
pzoom-cli
    └── pzoom-orchestrator (Scanner, Populator, Analyzer)
            ├── pzoom-analyzer (statement/expression analyzers, config)
            │       └── pzoom-code-info (type system)
            │               └── pzoom-str (string interning)
            └── mago-syntax (PHP parser - external git dependency)
```

### Type System

Modeled after Psalm's Union/Atomic pattern:
- `TUnion` - Union of atomic types (e.g., `int|string`, `Foo|null`)
- `TAtomic` - Single atomic type (TInt, TString, TNamedObject, TKeyedArray, etc.)

Located in `pzoom-code-info/src/`.

### String Interning

All strings are interned via `StrId` and `Interner` (in `pzoom-str`). The `Interner` uses interior mutability (`RwLock`) so `intern(&self)` works without `&mut self`.

### Analyzer Structure

Statement analyzers in `pzoom-analyzer/src/stmt/`:
- `if_analyzer.rs`, `while_analyzer.rs`, `foreach_analyzer.rs`, etc.

Expression analyzers in `pzoom-analyzer/src/expr/`:
- `variable_fetch_analyzer.rs`, `binop_analyzer.rs`, `assignment_analyzer.rs`, etc.

`FunctionAnalysisData` accumulates analysis state. `BlockContext` tracks variable types within scopes.

### Configuration

Reads Psalm XML config format (`psalm.xml`, `psalm.xml.dist`, `psalm.dist.xml`). Parser in `pzoom-analyzer/src/psalm_config.rs`.

## Testing

### Test Structure

Inference tests live at `tests/inference/<Suite>/<Case>/`. Each test case is a
directory containing:
- `input.php` - PHP code to analyze
- `output.txt` - Expected issues (optional; if missing, expects no errors)

### Test Runner Options

```
--reuse-codebase    Scan stubs once and reuse across tests (5x faster)
--release           Use optimized build (always recommended for full suite)
--update            Regenerate expected output files
--verbose, -v       Show detailed output
```

### Stubs

PHP built-in type stubs are in `stubs/`:
- Core stubs from Psalm (`CoreGenericFunctions.phpstub`, `SPL.phpstub`, etc.)
- Extension stubs from mago (`stubs/extensions/` - curl, json, pdo, etc.)

The test runner and CLI automatically scan stubs before user files.

## Key Files

- Type string formatting used in issues:
  - `crates/pzoom-code-info/src/t_atomic.rs`
  - `crates/pzoom-code-info/src/t_union.rs`
  - Issue text quality depends heavily on these; use interner-aware IDs when
    constructing user-facing issue strings.
- Unsupported expression handling:
  - `crates/pzoom-analyzer/src/expression_analyzer.rs`
  - Prefer emitting an analyzer issue (a dedicated kind, e.g. unrecognized
    expression) over panicking.
- Callable validation — confirm emitted issue kinds/messages match Psalm for
  docblock/signature mismatch, callable candidate validation, and invalid
  argument diagnostics:
  - `crates/pzoom-analyzer/src/expr/call/callable_validation.rs`
- Undefined method/property behavior:
  - Instance/static method calls:
    - `crates/pzoom-analyzer/src/expr/call/method_call_analyzer.rs`
    - `crates/pzoom-analyzer/src/expr/call/static_call_analyzer.rs`
  - Property fetches:
    - `crates/pzoom-analyzer/src/expr/fetch/instance_property_fetch_analyzer.rs`
    - `crates/pzoom-analyzer/src/expr/fetch/static_property_fetch_analyzer.rs`
- Interner/preloaded string constants:
  - `crates/pzoom-str/build.rs`

## Interner Rules

- Prefer `StrId::*` constants over `interner.intern("literal")` when a stable preloaded string exists.
- When adding strings to preload, check generated constant-name collisions in `format_identifier`.
- Known collision: `"serialize"` and `"__serialize"`; keep `"__serialize"` mapped to `MAGIC_SERIALIZE`.
- When adding magic methods, preload all PHP magic methods.

## External Dependencies

The mago PHP parser is pulled as a git dependency from `github.com/carthage-software/mago`. Mago crates use the path pattern `mago_syntax::ast::ast::*` for AST types.
