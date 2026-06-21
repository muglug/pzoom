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

## Stub augmentation of project classes

pzoom **augments** a project class with the magic members a stub declares: a
stub adds `@property`/`@method`/`@mixin` to a class without replacing what the
class itself declares (`register_class` keeps the real declaration as the base
and folds the stub's magic members in). So the generated `@property` lines above
apply to your real models.

The one requirement is a **magic getter** for pzoom to consult `@property`
through — pzoom only resolves a magic property on a class that has `__get` (and
`@method` needs `__call`), mirroring how the members actually exist at runtime.
Eloquent's base `Model` provides `__get`/`__set`/`__call`, so every model
qualifies; a plain class would need to declare them (or `@mixin` something that
does).

A stub can only *add* magic members — it can't override what the class declares
itself, and built-in stubs carry no magic members, so a project polyfill of a
stubbed name is unaffected.

Independently, any stub that **defines** a symbol the analyzed code is otherwise
missing works too — the common case for framework glue that only exists at
runtime.

## Running it

This directory isn't wired into pzoom's build (it references `illuminate/*`,
which pzoom doesn't depend on). To try it, drop it into a Laravel project's
`vendor/`, or point a small script at `LaravelStubProvider::getStubFiles()` from
the project root.
