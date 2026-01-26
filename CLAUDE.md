# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
            └── mago-syntax (PHP parser - external, lives in ../mago)
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

Inference tests are in `tests/inference/`. Each test is a directory containing:
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

## External Dependencies

The mago PHP parser is pulled as a git dependency from `github.com/carthage-software/mago`. Mago crates use the path pattern `mago_syntax::ast::ast::*` for AST types.
