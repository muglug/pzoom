# Codex Playbook: Updating Pzoom

This guide is for parity work between pzoom and Psalm/Hakana.

## Goals

- Match Psalm behavior for issues and type reasoning.
- Use Hakana as an implementation/reference aid when it already solved a similar problem.
- Avoid regressions in inference test expectations.

## Reference Repositories

- Psalm: `/Users/brownmatthew/git/psalm`
- Hakana core: `/Users/brownmatthew/git/hakana/hakana-core`

Use local source and tests from those repositories first.

## Typical Workflow

1. Reproduce and isolate
   - Run a focused test suite:
     - `cargo run --release -p pzoom-test-runner -- --reuse-codebase tests/inference/<Suite>`
   - If needed, run pzoom against Psalm:
     - `cargo run --release -p pzoom-cli -- analyze ../psalm`
2. Compare behavior
   - Find equivalent logic in Psalm first.
   - Find matching/ported logic in Hakana when available.
3. Implement minimally
   - Keep changes localized; avoid broad refactors during parity fixes.
4. Re-run focused checks
   - `cargo check`
   - Focused inference suite(s)
5. Re-run full inference before completion
   - `cargo run --release -p pzoom-test-runner -- --reuse-codebase`

## Common Parity Tasks

### Callable validation

- Main file: `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr/call/callable_validation.rs`
- Confirm emitted issue kinds/messages match Psalm for:
  - docblock/signature mismatch
  - callable candidate validation
  - invalid argument diagnostics

### Undefined method/property behavior

- Instance/static method calls:
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr/call/instance_call_analyzer.rs`
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr/call/static_call_analyzer.rs`
- Property fetches:
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr/fetch/instance_property_fetch_analyzer.rs`
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr/fetch/static_property_fetch_analyzer.rs`

### Unsupported expressions

- Prefer emitting an analyzer issue over panicking.
- Current pattern uses a dedicated issue kind (e.g. unrecognized expression) in:
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-analyzer/src/expr_analyzer.rs`

### Type ID formatting used in errors

- Issue text quality depends heavily on:
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-code-info/src/t_atomic.rs`
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-code-info/src/t_union.rs`
- Use interner-aware IDs when constructing user-facing issue strings.

## String Interning and StrId Constants

- Preloaded strings and generated constants come from:
  - `/Users/brownmatthew/git/pzoom/crates/pzoom-str/build.rs`
- Prefer `StrId::...` over repeated literal interning for canonical strings.
- When adding magic methods, include all PHP magic methods in preload list.
- Watch generated-name collisions in `format_identifier`.
  - Existing explicit disambiguation: `__serialize -> MAGIC_SERIALIZE`.

## Inference Test Conventions

- `tests/inference/<Suite>/<Case>/input.php` is the source input.
- `output.txt` is optional; missing file means no expected issues.
- Expected output can drift during parity work. Only update after verifying intended behavior.

## Practical Diffing Tips

- Compare by symbol name with `rg` in both repos.
- Grep for issue kinds to see where they are actually emitted.
- Grep for method/property resolution fallbacks in analyzer code paths.

## Definition of Done

- `cargo check` passes.
- Relevant focused suites pass.
- Full inference run is clean or all remaining output differences are explained and intentional.
- Changes are aligned with Psalm behavior (and Hakana where applicable).

