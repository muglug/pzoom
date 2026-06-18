# pzoom

A fast PHP static analyzer written in Rust — a port of [Psalm](https://github.com/vimeo/psalm).

Read the backstory: [From Psalm to Pzoom](https://mattbrown.dev/articles/from-psalm-to-pzoom).

> [!NOTE]
> This is not something I ever intend to support. Also, it is vibe-coded with no great care taken as to its overall quality. Caveat emptor.

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

pzoom aims to match Psalm's analysis, with one deliberate divergence:

- **Case-sensitive name resolution.** PHP and Psalm resolve class, function
  and method names case-insensitively. pzoom resolves them case-sensitively: a
  wrong-cased reference is reported as `UndefinedClass` /
  `UndefinedDocblockClass` / `UndefinedFunction` / `UndefinedMethod`, with the
  correctly-cased name suggested in the message (e.g. ``Class foo does not
  exist (incorrect casing of Foo)``). Runtime-truth checks still honor PHP
  semantics: `method_exists()` matches case-insensitively, and method
  *declarations* override parent methods case-insensitively.

## License

[MIT](LICENSE)
