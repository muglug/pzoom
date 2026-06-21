<?php

declare(strict_types=1);

namespace Pzoom;

/**
 * A stub provider contributes PHP stub files for pzoom to scan for type
 * information only — pzoom never analyzes or reports on stub files.
 *
 * pzoom's analyzer is a native binary (written in Rust) and cannot execute PHP,
 * so a framework integration that needs to *run* PHP to know the types — boot
 * the application, reflect over Eloquent models / container bindings / facades,
 * and so on — does that here, in PHP, before analysis begins. A provider
 * (optionally) generates stub files and returns their paths; the pzoom launcher
 * (`vendor/bin/pzoom`) runs every registered provider and hands the collected
 * paths to the native binary via `--stubs`.
 *
 * This is deliberately stubs-only: a provider can add type definitions but
 * cannot hook into analysis (pzoom's imperative hooks are compiled into the
 * binary). Most of what a Psalm plugin expresses through return-type providers
 * is representable as stub annotations (`@method`, `@property`, generics), so
 * stubs cover the common framework cases without executing user code mid-analysis.
 *
 * A package registers its provider(s) in composer.json:
 *
 *     "extra": {
 *         "pzoom": {
 *             "stub-providers": ["Vendor\\Package\\PzoomStubProvider"]
 *         }
 *     }
 *
 * Implementations must be constructible with no arguments.
 */
interface StubProvider
{
    /**
     * Return the stub files (or directories of stubs) pzoom should scan.
     *
     * A provider that ships fixed stubs returns their paths directly. A provider
     * that derives types at runtime generates files into `$cacheDir` — a
     * writable directory reserved for this provider — and returns those paths.
     * Returned paths may be absolute or relative to the project root; a returned
     * directory is scanned for its `.php` / `.phpstub` files.
     *
     * @param string $cacheDir writable directory the provider may generate into
     * @return list<string> stub file or directory paths
     */
    public function getStubFiles(string $cacheDir): array;
}
