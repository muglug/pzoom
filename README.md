# pzoom

A fast PHP static analyzer written in Rust — a port of [Psalm](https://github.com/vimeo/psalm).

Read the backstory: [From Psalm to Pzoom](https://mattbrown.dev/articles/from-psalm-to-pzoom).

> [!NOTE]
> This is not something I ever intend to support — Caveat Emptor.

## Building

Requires a recent stable [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/muglug/pzoom
cd pzoom
cargo build --release
```

The binary ends up at `target/release/pzoom`.

## Running

Run it from (or point it at) a project with a `psalm.xml`:

```bash
target/release/pzoom path/to/php/project
```

pzoom reads Psalm's XML configuration format (`psalm.xml`, `psalm.xml.dist` or
`psalm.dist.xml` in the project root), so an existing Psalm setup works as-is —
see the [Psalm configuration docs](https://psalm.dev/docs/running_psalm/configuration/).
`pzoom --help` lists the handful of CLI options (output format, thread count,
config path).

## Testing

```bash
# Inference tests (tests/inference/**)
cargo run --release -p pzoom-test-runner

# Unit tests
cargo test
```

## Differences to Psalm

pzoom aims to broadly match Psalm's analysis, with a few deliberate divergences:

- **Case-sensitive name resolution.** PHP and Psalm resolve class, function
  and method names case-insensitively. pzoom resolves them case-sensitively: a
  wrong-cased reference is reported as `UndefinedClass` /
  `UndefinedDocblockClass` / `UndefinedFunction` / `UndefinedMethod`, with the
  correctly-cased name suggested in the message (e.g. ``Class foo does not
  exist (incorrect casing of Foo)``). Runtime-truth checks still honor PHP
  semantics: `method_exists()` matches case-insensitively, and method
  *declarations* override parent methods case-insensitively.
- **ComplexMethod calculation** Pzoom's ComplexMethod metric is derived from the
  graph of connections between assignments within a function. Pzoom’s graph is larger
  than Psalm’s (it tracks more connections) and so we approximate ComplexMethod issues
  in Psalm.
- **Return-type mismatches reported per statement.** pzoom does not emit the
  function-level `MoreSpecificReturnType`, `InvalidNullableReturnType` or
  `InvalidFalsableReturnType`. A declared-vs-inferred return mismatch is reported
  at the offending `return` instead — `InvalidReturnStatement`,
  `NullableReturnStatement`, `FalsableReturnStatement` or
  `LessSpecificReturnStatement`. The function-level `InvalidReturnType` is kept
  only for the structural cases that have no single `return` to point at: an
  implicit fall-through ("not all code paths of … end in a return"), a body with
  no `return` statements at all, a `never` body that nonetheless returns, or a
  generator whose aggregated yield/return type is wrong. Unlike Psalm — which
  guards the per-`return` checks inside trait bodies and relies on the
  function-level check there — pzoom runs them in trait methods too: the declared
  return is localized to each using class (`self`/`static` bind to that class and
  the trait's `@template` params resolve to their `as` bound, mirroring how Psalm
  checks a generic trait), so a trait method's bad return is caught at the
  `return` without spurious `self`-vs-`static` or template-binding-width
  mismatches.

## License

[MIT](LICENSE)
