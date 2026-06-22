<?php

declare(strict_types=1);

namespace Pzoom\Laravel;

use Illuminate\Contracts\Console\Kernel;
use Illuminate\Contracts\Foundation\Application;
use Illuminate\Database\Eloquent\Model;
use Pzoom\StubProvider;
use ReflectionClass;

/**
 * Reference {@see StubProvider} for Laravel — the pzoom counterpart of
 * psalm-plugin-laravel's stub generation.
 *
 * It demonstrates the pattern that justifies a PHP pre-pass: pzoom's native
 * analyzer can't run PHP, so the knowledge that only exists once the framework
 * is booted (here, each Eloquent model's `$casts`) is gathered in PHP, before
 * analysis, and emitted as stub files the binary then scans.
 *
 * What it generates: for every model under `app/Models`, a `@property` line per
 * cast (mapped to its PHP type) plus an `@mixin` onto the query builder.
 *
 * pzoom *augments* a project class with the magic members a stub declares, so
 * these `@property` lines apply to your real models — the stub adds them without
 * disturbing the model's own code. The one requirement is that the class has a
 * magic getter for pzoom to consult `@property` through, which Eloquent's base
 * `Model` supplies (`__get`/`__set`/`__call`). A stub can only *add* magic
 * members; it can't replace what the class itself declares.
 *
 * Implementations are constructed with no arguments (see {@see StubProvider}).
 */
final class LaravelStubProvider implements StubProvider
{
    public function getStubFiles(string $cacheDir): array
    {
        $projectRoot = getcwd();
        if ($projectRoot === false) {
            return [];
        }

        $app = $this->bootApplication($projectRoot);
        if ($app === null) {
            // Not a Laravel project, or the app failed to boot — contribute
            // nothing rather than guess.
            return [];
        }

        $modelStub = $this->generateModelStubs($projectRoot, $cacheDir);

        return $modelStub === null ? [] : [$modelStub];
    }

    /**
     * Boot the Laravel application the way an Artisan command would, so the
     * container, config and casts are available to reflect over.
     */
    private function bootApplication(string $projectRoot): ?Application
    {
        $bootstrap = $projectRoot . '/bootstrap/app.php';
        if (!is_file($bootstrap)) {
            return null;
        }

        try {
            /** @var Application $app */
            $app = require $bootstrap;
            $app->make(Kernel::class)->bootstrap();

            return $app;
        } catch (\Throwable) {
            return null;
        }
    }

    /**
     * Emit one stub file declaring, per model namespace, a class carrying the
     * `@property` types its `$casts` imply.
     */
    private function generateModelStubs(string $projectRoot, string $cacheDir): ?string
    {
        $models = $this->discoverModels($projectRoot . '/app/Models');
        if ($models === []) {
            return null;
        }

        /** @var array<string, list<string>> $byNamespace */
        $byNamespace = [];
        foreach ($models as $fqcn) {
            $namespace = $this->namespaceOf($fqcn);
            $byNamespace[$namespace][] = $this->renderModelStub(
                $this->shortNameOf($fqcn),
                $this->modelProperties($fqcn),
            );
        }

        $php = "<?php\n";
        foreach ($byNamespace as $namespace => $classes) {
            $php .= "\nnamespace " . $namespace . " {\n" . implode("\n", $classes) . "\n}\n";
        }

        $path = $cacheDir . '/models.phpstub';
        file_put_contents($path, $php);

        return $path;
    }

    /**
     * The Eloquent model classes declared under `$dir`, found by reading each
     * file's declared class (via the tokenizer, no autoload side effects) and
     * keeping those that the autoloader confirms extend {@see Model}.
     *
     * @return list<class-string<Model>>
     */
    private function discoverModels(string $dir): array
    {
        if (!is_dir($dir)) {
            return [];
        }

        $models = [];
        $files = new \RecursiveIteratorIterator(
            new \RecursiveDirectoryIterator($dir, \FilesystemIterator::SKIP_DOTS),
        );
        foreach ($files as $file) {
            if (!$file instanceof \SplFileInfo || $file->getExtension() !== 'php') {
                continue;
            }
            $fqcn = $this->classInFile((string) $file);
            if ($fqcn !== null && class_exists($fqcn) && is_subclass_of($fqcn, Model::class)) {
                $models[] = $fqcn;
            }
        }
        sort($models);

        return $models;
    }

    /**
     * `@property` types for a model, derived from its declared `$casts` map.
     * Reflection reads the default value of `$casts` without instantiating.
     *
     * @param class-string<Model> $fqcn
     * @return array<string, string> column name => PHP type
     */
    private function modelProperties(string $fqcn): array
    {
        $casts = (new ReflectionClass($fqcn))->getDefaultProperties()['casts'] ?? [];
        if (!is_array($casts)) {
            return [];
        }

        $properties = [];
        foreach ($casts as $column => $cast) {
            if (is_string($column) && is_string($cast)) {
                $properties[$column] = $this->castToPhpType($cast);
            }
        }

        return $properties;
    }

    /**
     * Map a Laravel cast (`datetime`, `decimal:2`, an enum/`Castable` class-string, …)
     * to the PHP type pzoom should see for it.
     */
    private function castToPhpType(string $cast): string
    {
        // Recognized built-in casts first — `$base` drops any `:args` suffix.
        // (Checked before class detection so e.g. `datetime` isn't mistaken for
        // PHP's case-insensitive `DateTime` class.)
        $base = strtolower(explode(':', $cast, 2)[0]);
        $known = match ($base) {
            'int', 'integer', 'timestamp' => 'int',
            'real', 'float', 'double' => 'float',
            'decimal' => 'numeric-string',
            'string', 'encrypted' => 'string',
            'bool', 'boolean' => 'bool',
            'array', 'json', 'collection' => 'array',
            'object' => '\\stdClass',
            'date', 'datetime', 'immutable_date', 'immutable_datetime' => '\\Illuminate\\Support\\Carbon',
            default => null,
        };
        if ($known !== null) {
            return $known;
        }

        // Otherwise an enum or custom-cast (`Castable`) class string casts to
        // that class.
        if (class_exists($cast) || interface_exists($cast)) {
            return '\\' . ltrim($cast, '\\');
        }

        return 'mixed';
    }

    /**
     * @param array<string, string> $properties column name => PHP type
     */
    private function renderModelStub(string $shortName, array $properties): string
    {
        $lines = ['/**'];
        foreach ($properties as $column => $type) {
            $lines[] = ' * @property ' . $type . ' $' . $column;
        }
        $lines[] = ' * @mixin \\Illuminate\\Database\\Eloquent\\Builder';
        $lines[] = ' */';
        $doc = implode("\n", $lines);

        return $doc . "\nclass " . $shortName . " extends \\Illuminate\\Database\\Eloquent\\Model {}";
    }

    /**
     * The fully-qualified class name declared in a PHP file (its `namespace` +
     * first named `class`), read with the tokenizer — no autoloading or eval.
     */
    private function classInFile(string $path): ?string
    {
        $code = @file_get_contents($path);
        if ($code === false) {
            return null;
        }

        $tokens = \PhpToken::tokenize($code);
        $count = count($tokens);
        $namespace = '';
        for ($i = 0; $i < $count; $i++) {
            if ($tokens[$i]->is(T_NAMESPACE)) {
                $namespace = $this->readName($tokens, $i + 1);
            } elseif ($tokens[$i]->is(T_CLASS)) {
                $name = $this->readClassName($tokens, $i + 1);
                if ($name !== null) {
                    return $namespace === '' ? $name : $namespace . '\\' . $name;
                }
            }
        }

        return null;
    }

    /**
     * Read a (possibly qualified) name starting at `$start`, stopping at `;`/`{`.
     *
     * @param list<\PhpToken> $tokens
     */
    private function readName(array $tokens, int $start): string
    {
        $name = '';
        for ($i = $start, $n = count($tokens); $i < $n; $i++) {
            $token = $tokens[$i];
            if ($token->is([T_STRING, T_NAME_QUALIFIED, T_NAME_FULLY_QUALIFIED, T_NS_SEPARATOR])) {
                $name .= $token->text;
            } elseif (!$token->is(T_WHITESPACE)) {
                break;
            }
        }

        return trim($name, '\\');
    }

    /**
     * The class name token after `class`, or null for `Foo::class` / anonymous
     * classes (which have no name token here).
     *
     * @param list<\PhpToken> $tokens
     */
    private function readClassName(array $tokens, int $start): ?string
    {
        for ($i = $start, $n = count($tokens); $i < $n; $i++) {
            $token = $tokens[$i];
            if ($token->is(T_STRING)) {
                return $token->text;
            }
            if (!$token->is(T_WHITESPACE)) {
                return null;
            }
        }

        return null;
    }

    private function namespaceOf(string $fqcn): string
    {
        $pos = strrpos($fqcn, '\\');

        return $pos === false ? '' : substr($fqcn, 0, $pos);
    }

    private function shortNameOf(string $fqcn): string
    {
        $pos = strrpos($fqcn, '\\');

        return $pos === false ? $fqcn : substr($fqcn, $pos + 1);
    }
}
