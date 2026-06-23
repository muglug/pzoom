# pzoom

A fast PHP static analyzer written in Rust — a port of [Psalm](https://github.com/vimeo/psalm).

Read the backstory: [From Psalm to Pzoom](https://mattbrown.dev/articles/from-psalm-to-pzoom).

> [!NOTE]
> This is not something I ever intend to support. Also, it is vibe-coded with no great care taken as to its overall quality. Caveat emptor.

## Installing with Composer

PHP projects can install pzoom as a dev dependency and version-manage it
alongside everything else in `composer.json`:

```bash
composer require --dev muglug/pzoom
```

There's no Rust toolchain involved on your side. A Composer plugin fetches the
prebuilt native binary that matches your platform from the matching GitHub
release during `composer install` / `composer update`, and exposes it at
`vendor/bin/pzoom`:

```bash
vendor/bin/pzoom path/to/php/project
```

Each Composer version maps to a `vX.Y.Z` GitHub release, so pinning a constraint
(`"muglug/pzoom": "^0.1"`) pins the binary too.

Because the package ships a Composer plugin, Composer will ask you to allow it
on first install. To allow it non-interactively (e.g. in CI), add it to your
`composer.json`:

```json
{
    "config": {
        "allow-plugins": {
            "muglug/pzoom": true
        }
    }
}
```

Prebuilt binaries are published for Linux (`x86_64`, `aarch64`) and macOS
(Apple Silicon). On any other platform, build from source as below.

## Stub providers

pzoom's analyzer is a native binary and can't execute PHP, so a framework
integration that needs to *run* PHP to know the types — boot the app and reflect
over Eloquent models, container bindings, facades, and so on, the way
[psalm-plugin-laravel](https://github.com/psalm/psalm-plugin-laravel) does —
can't run inside analysis. Instead it runs as a **stub provider**: a small PHP
class that (optionally) generates stub files before analysis. `vendor/bin/pzoom`
runs every registered provider, then hands the binary their stubs via `--stubs`;
the binary scans them for type information only (never analyzing or reporting on
them).

This is deliberately stubs-only — a provider adds type definitions, it doesn't
hook into analysis. Most of what a Psalm plugin expresses through return-type
providers is representable as stub annotations (`@method`, `@property`,
generics), so stubs cover the common framework cases without executing user code
mid-analysis.

Write a provider by implementing `Pzoom\StubProvider`:

```php
use Pzoom\StubProvider;

final class MyStubProvider implements StubProvider
{
    public function getStubFiles(string $cacheDir): array
    {
        // Ship fixed stubs, or generate them here (boot the app, reflect, …)
        // and write into $cacheDir. Return the file/directory paths to scan.
        file_put_contents($cacheDir . '/models.phpstub', $generatedStub);
        return [$cacheDir . '/models.phpstub'];
    }
}
```

and registering it in the package's `composer.json` — pzoom discovers providers
from every installed package (and the root project) this way:

```json
{
    "extra": {
        "pzoom": {
            "stub-providers": ["Vendor\\Package\\MyStubProvider"]
        }
    }
}
```

Providers generate into `.pzoom/` in the project (add it to `.gitignore`); pzoom
never analyzes that directory. You can also pass stubs directly, bypassing the
provider machinery:

```bash
vendor/bin/pzoom --stubs path/to/stubs path/to/php/project
```

[`examples/pzoom-laravel`](examples/pzoom-laravel) is a worked reference provider
that boots a Laravel app and generates Eloquent model stubs from `$casts`.

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
