<?php
/**
 * Import Psalm's CallMap dictionaries into pzoom's dictionaries/ directory.
 *
 * Psalm composes a full CallMap_XY.php per PHP minor version (70..85) and
 * loads the one matching the analysis version (InternalCallMapHandler).
 * Shipping all eleven full maps would be ~20MB, so this emits:
 *
 *   dictionaries/callmap_85.json     — the newest composed map
 *   dictionaries/callmap_deltas.json — downgrade deltas (85→84, 84→83, …):
 *                                      {"84": {"set": {...}, "remove": [...]}}
 *   dictionaries/psalm_stub_functions.json
 *       — lowercased names of functions declared in Psalm's own stub files;
 *         for these, pzoom's stub storage (Psalm-derived docblocks) stays
 *         authoritative and the CallMap entry is not applied.
 *
 * Entry format: {"name": [["", return_type], [param_key, type], ...]} — an
 * ordered list so JSON object-order semantics don't matter. Param keys keep
 * Psalm's CallMap syntax ('name', 'name=', '&w_name', '...name', …).
 *
 * Usage: php tools/import_callmap.php <psalm-checkout>
 */

if ($argc < 2) {
    fwrite(STDERR, "usage: php tools/import_callmap.php <psalm-checkout>\n");
    exit(1);
}
$psalm = rtrim($argv[1], '/');
$out_dir = __DIR__ . '/../dictionaries';
@mkdir($out_dir);

const VERSIONS = [85, 84, 83, 82, 81, 80, 74, 73, 72, 71, 70];

/** @return array<string, list<array{string, string}>> */
function load_map(string $psalm, int $version): array
{
    /** @var array<string, array<int|string, string>> $raw */
    $raw = require "$psalm/dictionaries/CallMap_$version.php";
    $map = [];
    foreach ($raw as $name => $signature) {
        $entry = [];
        foreach ($signature as $key => $type) {
            // Key 0 is the return type; everything else is a param.
            $entry[] = [$key === 0 ? '' : (string)$key, $type];
        }
        $map[strtolower((string)$name)] = $entry;
    }
    return $map;
}

$newest = load_map($psalm, VERSIONS[0]);

// ---------------------------------------------------------------------------
// The runtime CallMap holds only what a stub file cannot express:
//  - functions whose signature differs across PHP versions (or that exist in
//    only some versions),
//  - functions with multiple definitions (arity variants `name'1`).
// Everything else is stable and lives in the stub files
// (tools/callmap_to_stubs.php ports those types into the stubs).
// ---------------------------------------------------------------------------
$varying = [];
foreach ($newest as $name => $_) {
    if (str_contains($name, "'")) {
        $varying[$name] = true;
        $varying[explode("'", $name)[0]] = true;
    }
}

$all_versions = [VERSIONS[0] => $newest];
foreach (array_slice(VERSIONS, 1) as $version) {
    $all_versions[$version] = load_map($psalm, $version);
}
$all_names = [];
foreach ($all_versions as $map) {
    foreach ($map as $name => $_) {
        $all_names[$name] = true;
    }
}
foreach (array_keys($all_names) as $name) {
    if (isset($varying[$name])) {
        continue;
    }
    $reference = null;
    foreach ($all_versions as $map) {
        $entry = $map[$name] ?? null;
        if ($reference === null) {
            $reference = $entry;
        } elseif ($entry !== $reference) {
            $varying[$name] = true;
            break;
        }
    }
}

$minimal_newest = array_intersect_key($newest, $varying);
ksort($minimal_newest);
file_put_contents(
    "$out_dir/callmap_85.json",
    json_encode($minimal_newest, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
);

// One downgrade-delta file per version: callmap_delta_84.json transforms the
// 85 map into the 84 map, and so on down the chain.
$higher = $minimal_newest;
foreach (array_slice(VERSIONS, 1) as $version) {
    $lower = array_intersect_key($all_versions[$version], $varying);

    $set = [];
    $remove = [];
    foreach ($lower as $name => $entry) {
        if (!isset($higher[$name]) || $higher[$name] !== $entry) {
            $set[$name] = $entry;
        }
    }
    foreach ($higher as $name => $_) {
        if (!isset($lower[$name])) {
            $remove[] = $name;
        }
    }
    sort($remove);
    ksort($set);
    file_put_contents(
        "$out_dir/callmap_delta_$version.json",
        json_encode(
            ['set' => (object)$set, 'remove' => $remove],
            JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE,
        ),
    );

    $higher = $lower;
}
@unlink("$out_dir/callmap_deltas.json");

// The stable remainder feeds the one-time stub port (not embedded in pzoom).
$stable = array_diff_key($newest, $varying);
ksort($stable);
file_put_contents(
    "$out_dir/callmap_stable_for_stubs.json",
    json_encode($stable, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE),
);
fwrite(STDERR, sprintf(
    "varying/multi-def runtime entries: %d; stable (stub-portable): %d\n",
    count($minimal_newest),
    count($stable),
));

// ---------------------------------------------------------------------------
// Functions declared in Psalm's own stubs (any file Psalm may load): their
// pzoom storage came from Psalm docblocks and stays authoritative.
// ---------------------------------------------------------------------------
$stub_functions = [];
$stub_files = array_merge(
    glob("$psalm/stubs/*.phpstub") ?: [],
    glob("$psalm/stubs/extensions/*.phpstub") ?: [],
);
foreach ($stub_files as $file) {
    $tokens = token_get_all(file_get_contents($file));
    $n = count($tokens);
    $depth = 0;
    $ns = '';
    $in_braced_ns = false;
    for ($i = 0; $i < $n; $i++) {
        $t = $tokens[$i];
        if (is_string($t)) {
            if ($t === '{') {
                $depth++;
            } elseif ($t === '}') {
                $depth--;
                if ($depth === 0 && $in_braced_ns) {
                    $in_braced_ns = false;
                    $ns = '';
                }
            }
            continue;
        }
        if ($t[0] === T_CURLY_OPEN || $t[0] === T_DOLLAR_OPEN_CURLY_BRACES) {
            $depth++;
            continue;
        }
        if ($t[0] === T_NAMESPACE && $depth === ($in_braced_ns ? 1 : 0)) {
            $ns = '';
            for ($j = $i + 1; $j < $n; $j++) {
                $tj = $tokens[$j];
                if (is_string($tj)) {
                    $in_braced_ns = $tj === '{';
                    break;
                }
                if (in_array($tj[0], [T_STRING, T_NAME_QUALIFIED, T_NS_SEPARATOR], true)) {
                    $ns .= $tj[1];
                }
            }
            continue;
        }
        if ($t[0] === T_FUNCTION && $depth === ($in_braced_ns ? 1 : 0)) {
            for ($j = $i + 1; $j < $n; $j++) {
                $tj = $tokens[$j];
                if (is_array($tj) && in_array($tj[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) {
                    continue;
                }
                if (is_string($tj) && $tj === '&') {
                    continue;
                }
                if (is_array($tj) && $tj[0] === T_STRING) {
                    $fqn = strtolower(($ns !== '' ? $ns . '\\' : '') . $tj[1]);
                    $stub_functions[$fqn] = true;
                }
                break;
            }
        }
    }
}
$stub_functions = array_keys($stub_functions);
sort($stub_functions);
file_put_contents(
    "$out_dir/psalm_stub_functions.json",
    json_encode($stub_functions, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES),
);

// ---------------------------------------------------------------------------
// Functions Psalm handles through dedicated machinery rather than the raw
// CallMap signature: ReturnTypeProvider classes (array_map, min, current, …)
// and the closure-argument special cases (ArgumentAnalyzer's
// PHP_NATIVE_NON_PUBLIC_CB + ArgumentsAnalyzer's ARRAY_FILTERLIKE). pzoom's
// richer stub docblocks (templates, conditional returns) stand in for that
// machinery, so the CallMap must not overwrite them.
// ---------------------------------------------------------------------------
$special = [];
foreach (glob("$psalm/src/Psalm/Internal/Provider/ReturnTypeProvider/*.php") ?: [] as $file) {
    $src = file_get_contents($file);
    if (preg_match('/function getFunctionIds\(\).*?return\s*\[(.*?)\];/s', $src, $m)) {
        preg_match_all('/[\'"]([a-z0-9_\\\\]+)[\'"]/i', $m[1], $names);
        foreach ($names[1] as $n) {
            $special[strtolower($n)] = true;
        }
    }
}
foreach ([
    ["$psalm/src/Psalm/Internal/Analyzer/Statements/Expression/Call/ArgumentAnalyzer.php",
     '/PHP_NATIVE_NON_PUBLIC_CB = \[(.*?)\];/s'],
    ["$psalm/src/Psalm/Internal/Analyzer/Statements/Expression/Call/ArgumentsAnalyzer.php",
     '/ARRAY_FILTERLIKE = \[(.*?)\];/s'],
] as [$file, $pattern]) {
    $src = file_get_contents($file);
    if (preg_match($pattern, $src, $m)) {
        preg_match_all('/[\'"]([a-z0-9_]+)[\'"]/i', $m[1], $names);
        foreach ($names[1] as $n) {
            $special[strtolower($n)] = true;
        }
    }
}
$special = array_keys($special);
sort($special);
file_put_contents(
    "$out_dir/psalm_special_functions.json",
    json_encode($special, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES),
);
fwrite(STDERR, count($special) . " special (provider/closure-arg) functions\n");

fwrite(STDERR, sprintf(
    "callmap_85: %d entries; deltas: %s; psalm stub functions: %d\n",
    count($newest),
    implode(', ', array_map(
        fn($v) => "$v(set " . count((array)$deltas[(string)$v]['set']) . ", rm " . count($deltas[(string)$v]['remove']) . ")",
        array_slice(VERSIONS, 1),
    )),
    count($stub_functions),
));
