<?php

declare(strict_types=1);

namespace Pzoom;

/**
 * Discovers and runs the {@see StubProvider}s registered by the project and its
 * Composer dependencies, returning the stub paths to hand to the native binary.
 *
 * Discovery is by Composer metadata: a package opts in with
 *
 *     "extra": { "pzoom": { "stub-providers": ["Fully\\Qualified\\Class"] } }
 *
 * in its composer.json. The installed set is read from
 * `vendor/composer/installed.json`, and the root project's own composer.json is
 * included too. Provider classes are autoloaded through the project's
 * `vendor/autoload.php`, which the launcher requires before calling this.
 *
 * Each provider may generate into its own subdirectory of `.pzoom/stubs/`; the
 * collected paths are de-duplicated and resolved to absolute paths.
 */
final class StubProviderRunner
{
    /**
     * @return list<string> absolute stub file / directory paths
     */
    public static function collect(string $projectRoot): array
    {
        $projectRoot = rtrim($projectRoot, '/\\');
        $providers = self::discoverProviderClasses($projectRoot);
        if ($providers === []) {
            return [];
        }

        $cacheRoot = $projectRoot . '/.pzoom/stubs';
        $paths = [];
        foreach ($providers as $class) {
            if (!class_exists($class) || !is_a($class, StubProvider::class, true)) {
                continue;
            }

            $cacheDir = $cacheRoot . '/' . self::slug($class);
            if (!is_dir($cacheDir)) {
                @mkdir($cacheDir, 0o777, true);
            }

            /** @var StubProvider $provider */
            $provider = new $class();
            foreach ($provider->getStubFiles($cacheDir) as $path) {
                if (!is_string($path)) {
                    continue;
                }
                $resolved = self::resolvePath($path, $projectRoot);
                if ($resolved !== null) {
                    $paths[$resolved] = true; // de-dupe by absolute path
                }
            }
        }

        return array_keys($paths);
    }

    /**
     * @return list<string>
     */
    private static function discoverProviderClasses(string $projectRoot): array
    {
        $classes = [];

        // The root project's own providers.
        self::collectFromComposerJson($projectRoot . '/composer.json', $classes);

        // Installed dependencies' providers — Composer copies each package's
        // `extra` into installed.json, so no per-package file reads are needed.
        $installed = $projectRoot . '/vendor/composer/installed.json';
        if (is_file($installed)) {
            $data = json_decode((string) file_get_contents($installed), true);
            // Composer 2 wraps packages under a "packages" key; Composer 1 was a
            // bare array.
            $packages = is_array($data) ? ($data['packages'] ?? $data) : [];
            if (is_array($packages)) {
                foreach ($packages as $package) {
                    if (is_array($package)) {
                        self::collectFromExtra($package['extra'] ?? null, $classes);
                    }
                }
            }
        }

        return array_values(array_unique($classes));
    }

    /**
     * @param list<string> $classes
     */
    private static function collectFromComposerJson(string $path, array &$classes): void
    {
        if (!is_file($path)) {
            return;
        }
        $data = json_decode((string) file_get_contents($path), true);
        if (is_array($data)) {
            self::collectFromExtra($data['extra'] ?? null, $classes);
        }
    }

    /**
     * @param mixed $extra
     * @param list<string> $classes
     */
    private static function collectFromExtra($extra, array &$classes): void
    {
        if (!is_array($extra)) {
            return;
        }
        $providers = $extra['pzoom']['stub-providers'] ?? null;
        if (is_string($providers)) {
            $providers = [$providers];
        }
        if (!is_array($providers)) {
            return;
        }
        foreach ($providers as $class) {
            if (is_string($class) && $class !== '') {
                $classes[] = ltrim($class, '\\');
            }
        }
    }

    private static function resolvePath(string $path, string $projectRoot): ?string
    {
        if ($path === '') {
            return null;
        }
        $absolute = self::isAbsolute($path) ? $path : $projectRoot . '/' . $path;
        $real = realpath($absolute);

        return $real !== false ? $real : null;
    }

    private static function isAbsolute(string $path): bool
    {
        return $path[0] === '/'
            || preg_match('#^[A-Za-z]:[\\\\/]#', $path) === 1;
    }

    private static function slug(string $class): string
    {
        return preg_replace('/[^A-Za-z0-9]+/', '_', $class) ?? 'provider';
    }
}
