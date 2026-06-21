# pzoom-laravel (reference stub provider)

A worked example of a [`Pzoom\StubProvider`](../../src/StubProvider.php) — the
pzoom counterpart of [psalm-plugin-laravel](https://github.com/psalm/psalm-plugin-laravel)'s
stub generation. It exists to show the shape of a real provider; you would
normally publish it as its own package (`muglug/pzoom-laravel`) that a Laravel
project requires.

## The pattern it demonstrates

pzoom's analyzer is a native binary and can't execute PHP, so anything that's
only knowable once the framework is booted has to be gathered **in PHP, before
analysis**. This provider does exactly that:

1. `bootApplication()` requires the project's `bootstrap/app.php` and bootstraps
   the kernel — the same thing an Artisan command does.
2. `discoverModels()` finds the Eloquent models under `app/Models` (reading each
   file's declared class with the tokenizer, no autoload side effects).
3. `modelProperties()` reflects each model's `$casts` and maps them to PHP types
   (`datetime → \Illuminate\Support\Carbon`, `decimal:2 → numeric-string`, an
   enum cast → that enum, …).
4. It writes a `.phpstub` and returns its path; `vendor/bin/pzoom` forwards it to
   the binary via `--stubs`.

Installed as a package it needs no wiring — pzoom discovers it from composer
metadata:

```json
{
    "extra": {
        "pzoom": {
            "stub-providers": ["Pzoom\\Laravel\\LaravelStubProvider"]
        }
    }
}
```

For a model with `protected $casts = ['id' => 'int', 'created_at' => 'datetime']`
it emits:

```php
/**
 * @property int $id
 * @property \Illuminate\Support\Carbon $created_at
 * @mixin \Illuminate\Database\Eloquent\Builder
 */
class User extends \Illuminate\Database\Eloquent\Model {}
```

## Current boundary: stubs vs. project classes

This is where a *stubs-only* system meets its edge, and the example is honest
about it. pzoom applies a stub's members only where the class is **otherwise
undefined**. It will not let a stub augment a class the project itself declares:
`register_class` mirrors Psalm, whose scanner "refuses to stub-override classes
from analyzed project dirs" — the project declaration wins wholesale.

Eloquent models are project classes, so the generated `@property` lines above
**do not currently take effect** when the models live in the analyzed code. They
apply only to models pzoom wouldn't otherwise see (e.g. ones shipped inside a
dependency). Psalm gets model magic-properties to work on project classes
through an imperative *property provider* (a hook that answers "does `$user`
have property `email`?" during analysis), not through a stub — and a
deliberately stubs-only provider system has no such hook.

So this provider is most valuable as a faithful illustration of the
boot-and-reflect pipeline, and as a concrete marker of the capability that would
make it fully effective. Two ways to get there:

- **Write annotations into the model files** (the approach `laravel-ide-helper`
  takes with its `--write` mode): then the `@property` lines live on the real
  class and pzoom honors them today. A provider can't do this (it only returns
  stubs), but a sibling command could.
- **Teach pzoom stub *augmentation*** of project classes — letting a stub add
  pseudo-members (`@property`/`@method`) to a class without replacing it. That's
  a focused `register_class` change, and would light up model stubs (and facade
  `@method` stubs, which hit the same gap) across the board.

What *does* work today, unchanged, is any stub that **defines** a symbol the
analyzed code is otherwise missing — the common case for framework glue that
only exists at runtime.

## Running it

This directory isn't wired into pzoom's build (it references `illuminate/*`,
which pzoom doesn't depend on). To try it, drop it into a Laravel project's
`vendor/`, or point a small script at `LaravelStubProvider::getStubFiles()` from
the project root.
