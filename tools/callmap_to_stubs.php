<?php
/**
 * Port stable CallMap entries into the stub files.
 *
 * The runtime CallMap (dictionaries/callmap_85.json + deltas) holds only
 * functions whose signatures vary across PHP versions or that have multiple
 * definitions. Everything else is stable and belongs in the stub files: this
 * tool rewrites each stable function's @param/@return(/@param-out) docblock
 * tags to the CallMap type, using the stub's parameter names positionally,
 * and drops leftover @template machinery (Psalm CallMap functions have none).
 *
 * Functions in psalm_stub_functions.json / psalm_special_functions.json are
 * untouched — their pzoom stubs stand in for Psalm's own stubs and dedicated
 * machinery.
 *
 * Usage: php tools/callmap_to_stubs.php
 */

const ROOT = __DIR__ . '/..';

// Reuse the token-based declaration parser from stub_graft.php (strip its
// CLI prologue and main section, keep the helpers).
$graft_src = file_get_contents(ROOT . '/tools/stub_graft.php');
$graft_src = preg_replace('/^.*?(?=const STUBS)/s', '', $graft_src);
$graft_src = preg_replace('/\/\/ -+\n\/\/ Main\n.*$/s', '', $graft_src);
$graft_src = preg_replace('/^const STUBS.*?\n/m', '', $graft_src);
$graft_src = preg_replace('/\$DRY_RUN = .*?\n|\$ONLY = null;\n|foreach \(\$argv as \$a\) \{.*?\n\}\n/s', '', $graft_src);
eval($graft_src);

$stable = json_decode(file_get_contents(ROOT . '/dictionaries/callmap_stable_for_stubs.json'), true);
$skip = array_fill_keys(array_merge(
    json_decode(file_get_contents(ROOT . '/dictionaries/psalm_stub_functions.json'), true),
    json_decode(file_get_contents(ROOT . '/dictionaries/psalm_special_functions.json'), true),
), true);

// Index every top-level stub function: lowercase fqn => [file, decl].
$files = [];
$it = new RecursiveIteratorIterator(new RecursiveDirectoryIterator(ROOT . '/stubs'));
foreach ($it as $f) {
    if ($f->isFile() && str_ends_with($f->getFilename(), '.phpstub')) {
        $files[] = $f->getPathname();
    }
}
sort($files);

$rewritten = 0;
$files_changed = 0;
foreach ($files as $path) {
    $code = file_get_contents($path);
    $decls = parse_decls($code);

    // Collect edits (offset-descending application).
    $edits = [];
    foreach ($decls as $decl) {
        if ($decl['kind'] !== 'function') {
            continue;
        }
        $name = $decl['fqn'];
        if (isset($skip[$name]) || !isset($stable[$name])) {
            continue;
        }
        $entry = $stable[$name];

        // Map CallMap entries: index 0 = return, the rest positional params.
        $return_type = null;
        $params = [];
        foreach ($entry as $i => [$key, $type]) {
            if ($i === 0 && $key === '') {
                $return_type = $type;
            } else {
                $params[] = [$key, $type];
            }
        }

        // Build the new tag lines using the stub's param names positionally.
        $tag_lines = [];
        foreach ($params as $position => [$key, $type]) {
            $stub_param = $decl['params'][$position] ?? null;
            if ($stub_param === null || $type === '') {
                continue; // CallMap optionals beyond the stub's (php-src) arity
            }
            $tag_lines[] = "@param $type $stub_param";
            if (str_starts_with($key, '&w_') || str_starts_with($key, '&rw_')) {
                $tag_lines[] = "@param-out $type $stub_param";
            }
        }
        if ($return_type !== null && $return_type !== '') {
            $tag_lines[] = "@return $return_type";
        }
        if (!$tag_lines) {
            continue;
        }

        // Rewrite the docblock: drop existing @param/@return/@param-out and
        // @template lines (with continuations), keep everything else, then
        // append the CallMap tags.
        $kept = [];
        if ($decl['doc'] !== null) {
            [$desc, $entries] = parse_docblock($decl['doc']);
            foreach ($desc as $line) {
                $kept[] = $line;
            }
            foreach ($entries as $tag_entry) {
                $tag = $tag_entry['tag'];
                if (in_array($tag, ['param', 'psalm-param', 'phpstan-param',
                    'return', 'psalm-return', 'phpstan-return',
                    'param-out', 'psalm-param-out'], true)
                    || str_contains($tag, 'template')
                ) {
                    continue;
                }
                foreach ($tag_entry['lines'] as $line) {
                    $kept[] = $line;
                }
            }
        }
        while ($kept && trim(end($kept)) === '') {
            array_pop($kept);
        }
        if ($kept) {
            $kept[] = '';
        }
        foreach ($tag_lines as $line) {
            $kept[] = $line;
        }

        $indent = indent_at($code, $decl['span_start']);
        $doc_text = "/**\n";
        foreach ($kept as $line) {
            $doc_text .= rtrim("$indent * $line") . "\n";
        }
        $doc_text .= "$indent */";

        if ($decl['doc'] !== null) {
            $doc_start = strpos($code, $decl['doc'], $decl['span_start']);
            $edits[] = [$doc_start, $doc_start + strlen($decl['doc']), $doc_text];
        } else {
            $edits[] = [$decl['span_start'], $decl['span_start'], $doc_text . "\n$indent"];
        }
        $rewritten++;
    }

    if (!$edits) {
        continue;
    }
    usort($edits, fn($a, $b) => $b[0] <=> $a[0]);
    foreach ($edits as [$start, $end, $text]) {
        $code = substr($code, 0, $start) . $text . substr($code, $end);
    }
    file_put_contents($path, $code);
    $files_changed++;
}

fwrite(STDERR, "rewrote $rewritten function docblocks across $files_changed stub files\n");
