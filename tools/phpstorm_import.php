<?php
/**
 * Import extension stubs from a jetbrains/phpstorm-stubs checkout into
 * stubs/extensions/<ext>.phpstub, normalized to this repo's conventions:
 *
 *  - JetBrains attributes are translated or kept:
 *      #[Pure] / #[Pure(true)]       -> kept verbatim (the declaration collector
 *                                       parses the Pure attribute natively)
 *      #[Deprecated(...)]            -> @deprecated [since X.Y] [reason]
 *      #[LanguageLevelTypeAware(...)]-> newest version's type (native on params/
 *                                       properties, @return tag for return position)
 *      #[PhpStormStubsElementAvailable(from)] -> kept (newest-PHP superset)
 *      #[PhpStormStubsElementAvailable(to)]   -> param dropped when to < 8.0
 *      #[Immutable]                  -> @psalm-immutable (classes) / @psalm-readonly (props)
 *      #[NoReturn]                   -> @return never (when no @return present)
 *      #[ArrayShape([...])]          -> @return array{...} (reported for review)
 *      #[ExpectedValues(...)], #[Language(...)] etc. -> dropped
 *  - Docblocks keep tags only (first line each), prose descriptions and @link
 *    dropped, inline HTML stripped. @since/@removed kept (version info).
 *  - Multi-file dirs concatenate; if any file is namespaced, every segment is
 *    wrapped in braced namespaces so global-ns segments stay global.
 *
 * Usage: php tools/phpstorm_import.php <phpstorm-stubs-dir> <ext-dir-name>...
 */

const JB_ATTRS = [
    'pure', 'deprecated', 'languageleveltypeaware', 'phpstormstubselementavailable',
    'immutable', 'noreturn', 'arrayshape', 'expectedvalues', 'language',
    'objectshape', 'availableversions',
];

$REPORT = [];
function report(string $msg): void
{
    global $REPORT;
    $REPORT[] = $msg;
    fwrite(STDERR, "[import] $msg\n");
}

if ($argc < 3) {
    fwrite(STDERR, "usage: php tools/phpstorm_import.php <phpstorm-stubs-dir> <ext>...\n");
    exit(1);
}
$srcRoot = rtrim($argv[1], '/');
$exts = array_slice($argv, 2);

// ---------------------------------------------------------------------------

/** Parse one attribute group "#[ ... ]" starting at token index $i (T_ATTRIBUTE).
 *  Returns [endIndexInclusive, attrs: list of [name, rawArgs|null]]. */
function parse_attr_group(array $toks, int $i): array
{
    $n = count($toks);
    $depth = 1; // the #[ itself
    $j = $i + 1;
    $parts = [];
    $cur = '';
    while ($j < $n && $depth > 0) {
        $t = $toks[$j];
        $txt = is_array($t) ? $t[1] : $t;
        if (is_string($t)) {
            if ($t === '[' || $t === '(') $depth++;
            elseif ($t === ']' || $t === ')') {
                $depth--;
                if ($depth === 0) break;
            } elseif ($t === ',' && $depth === 1) {
                $parts[] = $cur;
                $cur = '';
                $j++;
                continue;
            }
        } elseif (defined('T_ATTRIBUTE') && $t[0] === T_ATTRIBUTE) {
            $depth++;
        }
        $cur .= $txt;
        $j++;
    }
    if (trim($cur) !== '') $parts[] = $cur;

    $attrs = [];
    foreach ($parts as $p) {
        $p = trim($p);
        if (!preg_match('/^\\\\?([\w\\\\]+)\s*(\((.*)\))?$/s', $p, $m)) continue;
        $base = strtolower(substr(strrchr('\\' . $m[1], '\\'), 1));
        $attrs[] = [$base, isset($m[3]) ? trim($m[3]) : null];
    }
    return [$j, $attrs];
}

/** Newest type from a LanguageLevelTypeAware arg string. */
function newest_type(string $args): ?string
{
    // map entries '8.0' => 'Type'
    preg_match_all("/['\"]([\d.]+)['\"]\s*=>\s*['\"]([^'\"]*)['\"]/", $args, $m, PREG_SET_ORDER);
    $best = null;
    $bestV = '';
    foreach ($m as $e) {
        if ($best === null || version_compare($e[1], $bestV, '>')) {
            $best = $e[2];
            $bestV = $e[1];
        }
    }
    if ($best !== null) return $best;
    if (preg_match("/default\s*:\s*['\"]([^'\"]*)['\"]/", $args, $dm)) return $dm[1];
    return null;
}

/** Normalize a docblock: tags only, prose dropped, HTML stripped.
 *  Returns the surviving tag lines, or null if nothing survives. */
function normalize_docblock(string $doc): ?array
{
    $inner = preg_replace('/^\/\*\*|\*\/$/s', '', trim($doc));
    $lines = preg_split('/\r?\n/', $inner);
    $tags = [];
    $cur = null;
    foreach ($lines as $line) {
        $l = preg_replace('/^\s*\*( ?)/', '', rtrim($line));
        if (preg_match('/^@([a-zA-Z][\w-]*)/', ltrim($l), $m)) {
            if ($cur !== null) $tags[] = $cur;
            $cur = ['tag' => strtolower($m[1]), 'line' => trim($l)];
        }
        // continuation/prose lines dropped
    }
    if ($cur !== null) $tags[] = $cur;

    $keep = [];
    foreach ($tags as $t) {
        if (in_array($t['tag'], ['link', 'see', 'meta'], true)) continue;
        $line = preg_replace('/<[^>]+>/', '', $t['line']); // strip inline HTML
        $line = rtrim($line);
        if ($line === '@' . $t['tag'] && in_array($t['tag'], ['param', 'return'], true)) continue;
        $keep[] = $line;
    }
    if (!$keep) return null;
    return $keep;
}

/** Render docblock lines with the given indent. */
function render_docblock(array $lines, string $indent): string
{
    $out = "/**\n";
    foreach ($lines as $l) {
        $out .= rtrim("$indent * $l") . "\n";
    }
    $out .= "$indent */";
    return $out;
}

/**
 * Convert one upstream stub file's contents.
 * Returns [body (no <?php, no namespace stmt), namespaces: list of [ns, body]].
 */
function convert_file(string $code, string $label): array
{
    $toks = token_get_all($code);
    $n = count($toks);

    // Output segments per namespace: list of [nsName, text]
    $segments = [];
    $ns = '';
    $out = '';

    // Pending docblock tags queued by decl-level attributes.
    $queued = [];           // tag lines to add to the next decl's docblock
    $docSpan = null;        // [start, end] of last emitted docblock in $out
    $parenDepth = 0;
    $braceDepth = 0;
    $nsBaselines = [];      // brace depths at which braced namespaces opened
    $prevWasFunction = false; // last substantive token was `function` (keyword-named methods)

    $flushSegment = function () use (&$segments, &$ns, &$out) {
        if (trim($out) !== '') $segments[] = [$ns, $out];
        $out = '';
    };

    // Inject queued tags into the docblock at $docSpan, or synthesize one
    // right before offset $declStart in $out.
    $applyQueued = function (int $declStart) use (&$out, &$queued, &$docSpan) {
        if (!$queued) return;
        // Determine indentation of the decl line.
        $lineStart = strrpos(substr($out, 0, $declStart), "\n");
        $lineStart = $lineStart === false ? 0 : $lineStart + 1;
        preg_match('/^[ \t]*/', substr($out, $lineStart, $declStart - $lineStart), $m);
        $indent = $m[0];

        // @return never only when no @return already present.
        $existing = $docSpan !== null ? substr($out, $docSpan[0], $docSpan[1] - $docSpan[0]) : '';
        $queued = array_values(array_filter($queued, function ($q) use ($existing) {
            if (str_starts_with($q, '@return') && str_contains($existing, '@return')) return false;
            if (str_starts_with($q, '@deprecated') && str_contains($existing, '@deprecated')) return false;
            return true;
        }));
        if (!$queued) { $queued = []; return; }

        if ($docSpan !== null) {
            $doc = substr($out, $docSpan[0], $docSpan[1] - $docSpan[0]);
            $insert = '';
            foreach ($queued as $q) $insert .= "$indent * $q\n";
            $newDoc = preg_replace('/([ \t]*)\*\/$/', $insert . '$1*/', $doc, 1);
            $out = substr_replace($out, $newDoc, $docSpan[0], $docSpan[1] - $docSpan[0]);
        } else {
            $doc = render_docblock($queued, $indent) . "\n$indent";
            $out = substr_replace($out, $doc, $declStart, 0);
        }
        $queued = [];
        $docSpan = null;
    };

    $i = 0;
    while ($i < $n) {
        $t = $toks[$i];

        if (is_array($t)) {
            [$id, $txt] = $t;

            if ($id === T_OPEN_TAG || $id === T_CLOSE_TAG) { $i++; continue; }

            if ($id === T_CURLY_OPEN || $id === T_DOLLAR_OPEN_CURLY_BRACES) {
                $braceDepth++;
                $out .= $txt;
                $i++;
                continue;
            }

            if ($id === T_DECLARE && !$prevWasFunction) {
                // drop declare(strict_types=1); — segments concatenate, so it
                // would no longer be the first statement
                while ($i < $n && !(is_string($toks[$i]) && $toks[$i] === ';')) $i++;
                $i++;
                if ($i < $n && is_array($toks[$i]) && $toks[$i][0] === T_WHITESPACE) {
                    $toks[$i][1] = preg_replace('/^[ \t]*\n/', '', $toks[$i][1], 1);
                }
                continue;
            }

            if ($id === T_NAMESPACE && $parenDepth === 0 && !$prevWasFunction) {
                // flush previous segment, read new namespace
                $nsName = '';
                $j = $i + 1;
                while ($j < $n) {
                    $tt = $toks[$j];
                    if (is_string($tt)) break;
                    if (in_array($tt[0], [T_STRING, T_NAME_QUALIFIED, T_NS_SEPARATOR], true)) $nsName .= $tt[1];
                    elseif (!in_array($tt[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) break;
                    $j++;
                }
                // consume to ';' or '{'
                while ($j < $n && !(is_string($toks[$j]) && ($toks[$j] === ';' || $toks[$j] === '{'))) $j++;
                $braced = $j < $n && $toks[$j] === '{';
                $flushSegment();
                $ns = $nsName;
                if ($braced) {
                    // The namespace's own brace is not emitted (segments are
                    // re-wrapped on output); remember the depth it opened at so
                    // its matching '}' can be recognized and skipped too.
                    $nsBaselines[] = $braceDepth;
                }
                $i = $j + 1;
                continue;
            }

            if ($id === T_USE && $parenDepth === 0 && !$prevWasFunction) {
                // peek the imported name
                $j = $i + 1;
                $name = '';
                while ($j < $n) {
                    $tt = $toks[$j];
                    if (is_string($tt)) break;
                    if (in_array($tt[0], [T_STRING, T_NAME_QUALIFIED, T_NS_SEPARATOR], true)) { $name .= $tt[1]; $j++; continue; }
                    if ($tt[0] === T_WHITESPACE) { $j++; continue; }
                    break;
                }
                if (str_starts_with(strtolower(ltrim($name, '\\')), 'jetbrains\\')) {
                    while ($i < $n && !(is_string($toks[$i]) && $toks[$i] === ';')) $i++;
                    $i++; // past ';'
                    // swallow following newline
                    if ($i < $n && is_array($toks[$i]) && $toks[$i][0] === T_WHITESPACE) {
                        $toks[$i][1] = preg_replace('/^\n/', '', $toks[$i][1], 1);
                    }
                    continue;
                }
                $out .= $txt;
                $i++;
                continue;
            }

            if (defined('T_ATTRIBUTE') && $id === T_ATTRIBUTE) {
                [$end, $attrs] = parse_attr_group($toks, $i);
                $jb = [];
                $foreign = [];
                foreach ($attrs as $a) {
                    if (in_array($a[0], JB_ATTRS, true)) $jb[] = $a;
                    else $foreign[] = $a;
                }
                if ($foreign && !$jb) {
                    // keep group verbatim
                    for ($k = $i; $k <= $end; $k++) $out .= is_array($toks[$k]) ? $toks[$k][1] : $toks[$k];
                    $i = $end + 1;
                    continue;
                }
                if ($foreign && $jb) {
                    report("$label: mixed JetBrains/foreign attribute group — foreign attrs dropped: " . json_encode($foreign));
                }

                $dropParam = false;
                $typeOverride = null;
                $keepAttrs = [];
                foreach ($jb as [$name, $args]) {
                    switch ($name) {
                        case 'pure':
                            // kept: the declaration collector parses #[Pure] natively
                            $keepAttrs[] = ($args === null || trim($args) === '')
                                ? 'Pure'
                                : "Pure($args)";
                            break;
                        case 'deprecated':
                            $tag = '@deprecated';
                            if ($args !== null) {
                                if (preg_match("/since\s*:\s*['\"]([^'\"]+)['\"]/", $args, $m)) $tag .= ' ' . $m[1];
                                if (preg_match("/reason\s*:\s*['\"]((?:[^'\"\\\\]|\\\\.)*)['\"]/", $args, $m)) {
                                    $tag .= ' ' . preg_replace('/\s+/', ' ', stripslashes($m[1]));
                                } elseif (preg_match("/^\s*['\"]((?:[^'\"\\\\]|\\\\.)*)['\"]/", $args, $m)) {
                                    $tag .= ' ' . preg_replace('/\s+/', ' ', stripslashes($m[1]));
                                }
                            }
                            $queued[] = rtrim($tag);
                            break;
                        case 'languageleveltypeaware':
                            $typeOverride = $args !== null ? newest_type($args) : null;
                            break;
                        case 'phpstormstubselementavailable':
                            if ($args !== null && preg_match("/to\s*:\s*['\"]([\d.]+)['\"]/", $args, $m)
                                && version_compare($m[1], '8.0', '<')) {
                                if ($parenDepth > 0) {
                                    $dropParam = true;
                                    report("$label: dropped pre-8.0 param (to: {$m[1]})");
                                } else {
                                    report("$label: decl-level ElementAvailable(to: {$m[1]}) — kept, review");
                                }
                            }
                            if ($args !== null && preg_match("/from\s*:\s*['\"]([\d.]+)['\"]/", $args, $m)
                                && $parenDepth === 0 && version_compare($m[1], '7.0', '>=')) {
                                $queued[] = "@since {$m[1]} PHP";
                            }
                            break;
                        case 'immutable':
                            $queued[] = '@psalm-immutable';
                            break;
                        case 'noreturn':
                            $queued[] = '@return never';
                            break;
                        case 'arrayshape':
                            report("$label: #[ArrayShape] dropped — convert by hand: " . preg_replace('/\s+/', ' ', substr((string)$args, 0, 120)));
                            break;
                        default:
                            break; // expectedvalues, language, ... dropped silently
                    }
                }

                $i = $end + 1;
                if ($keepAttrs) {
                    // re-emit kept attributes (e.g. #[Pure]); docblock stays attached
                    $out .= '#[' . implode(', ', $keepAttrs) . ']';
                } elseif ($i < $n && is_array($toks[$i]) && $toks[$i][0] === T_WHITESPACE) {
                    // swallow whitespace right after a fully-dropped attribute group
                    if ($parenDepth > 0) {
                        $toks[$i][1] = ltrim($toks[$i][1]);
                    } else {
                        $toks[$i][1] = preg_replace('/^[ \t]*\n/', '', $toks[$i][1], 1);
                    }
                }

                if ($dropParam) {
                    // consume tokens to the next top-level ',' (inclusive) or ')' (exclusive)
                    $pd = 0;
                    while ($i < $n) {
                        $tt = $toks[$i];
                        if (is_string($tt)) {
                            if ($tt === '(' || $tt === '[') $pd++;
                            elseif ($tt === ']') $pd--;
                            elseif ($tt === ')') {
                                if ($pd === 0) break;
                                $pd--;
                            } elseif ($tt === ',' && $pd === 0) { $i++; break; }
                        }
                        $i++;
                    }
                    continue;
                }

                if ($typeOverride !== null) {
                    if ($parenDepth > 0) {
                        // param: drop existing type tokens up to the variable
                        while ($i < $n) {
                            $tt = $toks[$i];
                            if (is_array($tt) && $tt[0] === T_VARIABLE) break;
                            if (is_string($tt) && ($tt === ')' || $tt === ',')) break; // safety
                            if (is_string($tt) && ($tt === '&')) break;                // by-ref marker kept
                            if (is_array($tt) && $tt[0] === T_ELLIPSIS) break;          // variadic kept
                            $i++;
                        }
                        $out .= $typeOverride . ' ';
                        continue;
                    }
                    // decl level: property type or return type
                    $k = $i;
                    $isProp = false;
                    while ($k < $n) {
                        $tt = $toks[$k];
                        if (is_array($tt) && $tt[0] === T_FUNCTION) break;
                        if (is_array($tt) && $tt[0] === T_VARIABLE) { $isProp = true; break; }
                        if (is_string($tt) && ($tt === ';' || $tt === '{')) break;
                        $k++;
                    }
                    if ($isProp) {
                        // drop type tokens before the property variable
                        while ($i < $n) {
                            $tt = $toks[$i];
                            if (is_array($tt) && $tt[0] === T_VARIABLE) break;
                            if (is_array($tt) && in_array($tt[0], [T_PUBLIC, T_PROTECTED, T_PRIVATE, T_STATIC, T_READONLY, T_VAR, T_WHITESPACE], true)) {
                                $out .= $tt[1];
                                $i++;
                                continue;
                            }
                            $i++; // type token dropped
                        }
                        $out .= $typeOverride . ' ';
                        // emitted before variable; loop continues at T_VARIABLE
                        continue;
                    }
                    $queued[] = "@return $typeOverride";
                    continue;
                }
                continue;
            }

            if ($id === T_DOC_COMMENT) {
                $lines = normalize_docblock($txt);
                if ($lines === null) {
                    $i++;
                    // swallow trailing newline of the dropped docblock
                    if ($i < $n && is_array($toks[$i]) && $toks[$i][0] === T_WHITESPACE) {
                        $toks[$i][1] = preg_replace('/^[ \t]*\n/', '', $toks[$i][1], 1);
                    }
                    $docSpan = null;
                    continue;
                }
                // indentation = current line indent in output
                $lineStart = strrpos($out, "\n");
                $indent = $lineStart === false ? '' : (preg_match('/^([ \t]*)$/', substr($out, $lineStart + 1), $m) ? $m[1] : '');
                $doc = render_docblock($lines, $indent);
                $docSpan = [strlen($out), strlen($out) + strlen($doc)];
                $out .= $doc;
                $i++;
                continue;
            }

            // decl keywords: attach queued tags
            if ($queued && $parenDepth === 0
                && in_array($id, [T_FUNCTION, T_CLASS, T_INTERFACE, T_TRAIT, T_ENUM, T_CONST, T_VARIABLE,
                    T_PUBLIC, T_PROTECTED, T_PRIVATE, T_STATIC, T_FINAL, T_ABSTRACT, T_READONLY], true)) {
                $applyQueued(strlen($out));
            }

            if ($id === T_COMMENT) {
                // keep line/section comments
                $out .= $txt;
                $i++;
                continue;
            }

            $out .= $txt;
            if (!in_array($id, [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) {
                $prevWasFunction = $id === T_FUNCTION;
            }
            $i++;
            continue;
        }

        // plain string token
        if ($t === '(') $parenDepth++;
        elseif ($t === ')') $parenDepth = max(0, $parenDepth - 1);
        elseif ($t === '{') $braceDepth++;
        elseif ($t === '}') {
            if ($nsBaselines && $braceDepth === end($nsBaselines)) {
                // closes a braced namespace whose '{' we never emitted
                array_pop($nsBaselines);
                $flushSegment();
                $ns = '';
                $i++;
                continue;
            }
            $braceDepth = max(0, $braceDepth - 1);
        }
        if ($t !== '&') $prevWasFunction = false; // & survives for `function &name()`
        $out .= $t;
        $i++;
    }

    $flushSegment();
    return $segments;
}

// ---------------------------------------------------------------------------

foreach ($exts as $ext) {
    $dir = "$srcRoot/$ext";
    if (!is_dir($dir)) {
        report("missing upstream dir: $ext");
        continue;
    }
    $files = [];
    $it = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($dir));
    foreach ($it as $f) {
        if ($f->isFile() && str_ends_with($f->getFilename(), '.php')
            && !str_contains($f->getFilename(), '.phpstorm.meta.')) {
            $files[] = $f->getPathname();
        }
    }
    sort($files);
    if (!$files) {
        report("no source files: $ext");
        continue;
    }

    $GLOBALS['__pending_ns_close'] = [];
    $allSegments = [];
    foreach ($files as $f) {
        $code = file_get_contents($f);
        foreach (convert_file($code, "$ext/" . basename($f)) as $seg) {
            $allSegments[] = $seg;
        }
    }

    $usesNs = (bool)array_filter($allSegments, fn($s) => $s[0] !== '');
    $outName = strtolower(str_replace(' ', '-', $ext));
    $target = __DIR__ . "/../stubs/extensions/$outName.phpstub";

    $body = "<?php\n";
    if (!$usesNs) {
        foreach ($allSegments as [$nsName, $text]) {
            $body .= "\n" . trim($text, "\n") . "\n";
        }
    } else {
        foreach ($allSegments as [$nsName, $text]) {
            $text = trim($text, "\n");
            if ($text === '') continue;
            $header = $nsName === '' ? 'namespace {' : "namespace $nsName {";
            // indent content by 4
            $indented = implode("\n", array_map(
                fn($l) => $l === '' ? '' : "    $l",
                explode("\n", $text)
            ));
            $body .= "\n$header\n$indented\n}\n";
        }
    }

    // ?mixed is invalid (mixed already includes null) — appears in upstream
    // via LanguageLevelTypeAware substitution next to an existing '?'.
    $body = preg_replace('/\?\s*mixed\b/', 'mixed', $body);

    file_put_contents($target, $body);
    report("wrote $outName.phpstub (" . count($files) . " source files, " . strlen($body) . " bytes)");
}

fwrite(STDERR, "\n" . count($REPORT) . " report lines\n");
