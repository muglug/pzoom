<?php
/**
 * Stub migration tool: graft Psalm-authoritative docblocks from root stub files
 * onto the JetBrains-derived signature stubs in stubs/extensions/, then remove
 * the migrated declarations from the root files.
 *
 * Policy:
 *  - Extension-file signatures (param names + native types) are preserved verbatim.
 *  - Docblocks merge per tag-group: Psalm wins for any group it provides
 *    (param/$x, param-out/$x, return, var/$x, templates, implements/extends,
 *    purity, exact-name tags); extension-only tags are retained.
 *  - Classes are never replaced wholesale: per-member docblock grafts only.
 *    Psalm-only members are appended; everything ambiguous is reported.
 *  - Psalm docblock param references are positionally remapped to the extension
 *    signature's param names when they differ.
 *
 * Usage: php tools/stub_graft.php [--dry-run] [--only=<RootFile.phpstub>]
 */

const STUBS = __DIR__ . '/../stubs';

$DRY_RUN = in_array('--dry-run', $argv, true);
$ONLY = null;
foreach ($argv as $a) {
    if (str_starts_with($a, '--only=')) $ONLY = substr($a, strlen('--only='));
}

// Root files in graft order: unconditional core files first, then version
// overlays oldest-to-newest so the newest overlay's docblocks win.
$ROOT_ORDER = [
    'CoreGenericClasses.phpstub',
    'CoreGenericIterators.phpstub',
    'CoreImmutableClasses.phpstub',
    'CoreGenericAttributes.phpstub',
    'CoreGenericFunctions.phpstub',
    'SPL.phpstub',
    'Reflection.phpstub',
    'Php74.phpstub',
    'Php80.phpstub',
    'Php81.phpstub',
    'Php82.phpstub',
    'Php84.phpstub',
    'Php85.phpstub',
];

// Targets for symbols that have no duplicate in extensions/ today.
// Symbol (lowercase fqn) => [target extension file, optional "@since X.Y PHP" marker]
$RELOCATIONS = [
    'bcdiv' => ['bcmath.phpstub', null],
    'ldap_escape' => ['ldap.phpstub', null],
    'db2_escape_string' => ['ibm_db2.phpstub', null],
    'cubrid_real_escape_string' => ['cubrid.phpstub', null],
    'pg_escape_bytea' => ['pgsql.phpstub', null],
    'pg_escape_identifier' => ['pgsql.phpstub', null],
    'pg_escape_literal' => ['pgsql.phpstub', null],
    'pg_escape_string' => ['pgsql.phpstub', null],
    'create_function' => ['standard.phpstub', null],
    'imap\\connection' => ['imap.phpstub', '8.1'],
    'curlsharepersistenthandle' => ['curl.phpstub', '8.5'],
];

$REPORT = [];
function report(string $level, string $msg): void
{
    global $REPORT;
    $REPORT[] = [$level, $msg];
    fwrite(STDERR, sprintf("[%s] %s\n", $level, $msg));
}

// ---------------------------------------------------------------------------
// Tokenized declaration extraction (byte-offset based)
// ---------------------------------------------------------------------------

/**
 * Parse a stub file into top-level declarations with byte spans.
 *
 * Each declaration:
 *   kind: class|interface|trait|enum|function
 *   name: original-case short name
 *   fqn:  lowercase namespaced name
 *   doc:  docblock text or null
 *   span_start: byte offset where the decl (incl. docblock/attributes) starts
 *   span_end:   byte offset one past the decl end (closing brace or ;)
 *   params: ordered param names (functions only)
 *   members: for classlikes, list of member arrays (see parse_members)
 *   body_open / body_close: byte offsets of the classlike's braces
 *   head: declaration header text (between span start and body_open)
 */
function parse_decls(string $code): array
{
    $tokens = token_get_all($code);
    $n = count($tokens);

    // Annotate tokens with byte offsets.
    $offs = [];
    $pos = 0;
    for ($i = 0; $i < $n; $i++) {
        $offs[$i] = $pos;
        $pos += strlen(is_array($tokens[$i]) ? $tokens[$i][1] : $tokens[$i]);
    }
    $offs[$n] = $pos;

    $ns = '';
    $depth = 0;
    $inBracedNs = false; // inside `namespace X { ... }` — top level is depth 1
    $decls = [];
    // Offset just past the previous top-level decl (or file header): candidate
    // region for the next decl's docblock/attributes.
    $regionStart = 0;

    $skipWs = function (int $j) use ($tokens, $n) {
        while ($j < $n && is_array($tokens[$j])
            && in_array($tokens[$j][0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) {
            $j++;
        }
        return $j;
    };

    for ($i = 0; $i < $n; $i++) {
        $tok = $tokens[$i];
        if (is_string($tok)) {
            if ($tok === '{') $depth++;
            elseif ($tok === '}') {
                $depth--;
                if ($depth === 0 && $inBracedNs) {
                    // end of a braced namespace block
                    $inBracedNs = false;
                    $ns = '';
                    $regionStart = $offs[$i] + 1;
                }
            }
            continue;
        }
        [$id, $text] = $tok;
        if ($id === T_CURLY_OPEN || $id === T_DOLLAR_OPEN_CURLY_BRACES) { $depth++; continue; }

        if ($id === T_NAMESPACE && $depth === ($inBracedNs ? 1 : 0)) {
            $nsName = '';
            for ($j = $i + 1; $j < $n; $j++) {
                $t = $tokens[$j];
                if (is_string($t)) break;
                if (in_array($t[0], [T_STRING, T_NAME_QUALIFIED, T_NS_SEPARATOR], true)) $nsName .= $t[1];
                elseif (!in_array($t[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) break;
            }
            $ns = $nsName;
            // Namespace statement ends the leading region.
            for ($j = $i + 1; $j < $n; $j++) {
                if (is_string($tokens[$j]) && ($tokens[$j] === ';' || $tokens[$j] === '{')) {
                    $inBracedNs = $tokens[$j] === '{';
                    $regionStart = $offs[$j] + 1;
                    break;
                }
            }
            continue;
        }

        $topDepth = $inBracedNs ? 1 : 0;
        $isClasslike = $depth === $topDepth && in_array($id, [T_CLASS, T_INTERFACE, T_TRAIT, T_ENUM], true);
        $isFunction = $depth === $topDepth && $id === T_FUNCTION;
        if (!$isClasslike && !$isFunction) continue;

        // Skip ::class, new class, and anonymous declarations.
        $prev = null;
        for ($j = $i - 1; $j >= 0; $j--) {
            $t = $tokens[$j];
            if (is_array($t) && in_array($t[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) continue;
            $prev = $t;
            break;
        }
        if (is_array($prev) && in_array($prev[0], [T_DOUBLE_COLON, T_NEW], true)) continue;

        // Find name token.
        $nameTokIdx = null;
        for ($j = $i + 1; $j < $n; $j++) {
            $t = $tokens[$j];
            if (is_array($t) && in_array($t[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) continue;
            if (is_string($t) && $t === '&') continue;
            if (is_array($t) && $t[0] === T_STRING) $nameTokIdx = $j;
            break;
        }
        if ($nameTokIdx === null) continue; // closure/anonymous

        $name = $tokens[$nameTokIdx][1];
        $fqn = strtolower(($ns !== '' ? $ns . '\\' : '') . $name);

        // Walk back from the keyword over modifiers (abstract/final/readonly),
        // attributes, and the docblock to find span start.
        [$spanStart, $doc] = decl_lead($tokens, $offs, $i, $regionStart, $code);

        if ($isFunction) {
            [$params, $endOff] = function_tail($tokens, $offs, $nameTokIdx, $code);
            $decls[] = [
                'kind' => 'function', 'name' => $name, 'fqn' => $fqn,
                'doc' => $doc, 'span_start' => $spanStart, 'span_end' => $endOff,
                'params' => $params,
            ];
            $regionStart = $endOff;
            $i = token_at_offset($offs, $n, $endOff);
            // depth unchanged: function_tail consumed its braces
        } else {
            $kind = [T_CLASS => 'class', T_INTERFACE => 'interface', T_TRAIT => 'trait', T_ENUM => 'enum'][$id];
            // Find body open brace.
            $bodyOpen = null;
            for ($j = $nameTokIdx + 1; $j < $n; $j++) {
                if (is_string($tokens[$j]) && $tokens[$j] === '{') { $bodyOpen = $j; break; }
            }
            $bodyClose = match_brace($tokens, $bodyOpen);
            $endOff = $offs[$bodyClose] + 1;
            $decls[] = [
                'kind' => $kind, 'name' => $name, 'fqn' => $fqn,
                'doc' => $doc, 'span_start' => $spanStart, 'span_end' => $endOff,
                'head' => substr($code, $spanStart, $offs[$bodyOpen] - $spanStart),
                'body_open' => $offs[$bodyOpen], 'body_close' => $offs[$bodyClose],
                'members' => parse_members($tokens, $offs, $bodyOpen, $bodyClose, $code),
            ];
            $regionStart = $endOff;
            $i = $bodyClose;
            // we consumed the braces ourselves; depth stays balanced
        }
    }

    return $decls;
}

/** Find the last token index whose offset < $off (resync after manual scans). */
function token_at_offset(array $offs, int $n, int $off): int
{
    $lo = 0;
    $hi = $n - 1;
    while ($lo < $hi) {
        $mid = intdiv($lo + $hi + 1, 2);
        if ($offs[$mid] < $off) $lo = $mid;
        else $hi = $mid - 1;
    }
    return $lo;
}

/**
 * Walk back from a declaration keyword over modifiers/attributes/docblock.
 * Returns [span_start_offset, docblock_text|null].
 */
function decl_lead(array $tokens, array $offs, int $kwIdx, int $regionStart, string $code): array
{
    $start = $kwIdx;
    $doc = null;
    $docIdx = null;
    for ($j = $kwIdx - 1; $j >= 0 && $offs[$j] >= $regionStart; $j--) {
        $t = $tokens[$j];
        if (is_array($t)) {
            if ($t[0] === T_WHITESPACE) continue;
            if (in_array($t[0], [T_ABSTRACT, T_FINAL, T_READONLY, T_STATIC, T_PUBLIC, T_PROTECTED, T_PRIVATE, T_VAR, T_CLASS], true)) {
                $start = $j;
                continue;
            }
            if ($t[0] === T_DOC_COMMENT) {
                $doc = $t[1];
                $docIdx = $j;
                $start = $j;
                break; // docblock terminates the lead
            }
            if ($t[0] === T_COMMENT) { $start = $j; continue; }
            break;
        } else {
            if ($t === ']') {
                // Closing of an attribute group: scan back to the matching #[,
                // accepting any tokens inside.
                $bd = 1;
                $k = $j - 1;
                while ($k >= 0 && $bd > 0) {
                    $tt = $tokens[$k];
                    if (is_string($tt)) {
                        if ($tt === ']') $bd++;
                        elseif ($tt === '[') $bd--;
                    } elseif (defined('T_ATTRIBUTE') && $tt[0] === T_ATTRIBUTE) {
                        $bd--;
                    }
                    if ($bd === 0) break;
                    $k--;
                }
                if ($k < 0 || $offs[$k] < $regionStart) break;
                $start = $k;
                $j = $k; // continue walking back from before the attribute
                continue;
            }
            break;
        }
    }
    return [$offs[$start], $doc];
}

/** From a function's name token, collect param names and the end offset. */
function function_tail(array $tokens, array $offs, int $nameTokIdx, string $code): array
{
    $n = count($tokens);
    // Param list.
    $params = [];
    $parenOpen = null;
    for ($j = $nameTokIdx + 1; $j < $n; $j++) {
        if (is_string($tokens[$j]) && $tokens[$j] === '(') { $parenOpen = $j; break; }
    }
    $pd = 0;
    $end = null;
    for ($j = $parenOpen; $j < $n; $j++) {
        $t = $tokens[$j];
        if (is_string($t)) {
            if ($t === '(') $pd++;
            elseif ($t === ')') { $pd--; if ($pd === 0) { $end = $j; break; } }
        } elseif ($t[0] === T_VARIABLE && $pd === 1) {
            $params[] = $t[1];
        }
    }
    // Body or ;
    for ($j = $end + 1; $j < $n; $j++) {
        $t = $tokens[$j];
        if (is_string($t)) {
            if ($t === ';') return [$params, $offs[$j] + 1];
            if ($t === '{') {
                $close = match_brace($tokens, $j);
                return [$params, $offs[$close] + 1];
            }
        }
    }
    throw new RuntimeException('unterminated function');
}

function match_brace(array $tokens, int $openIdx): int
{
    $n = count($tokens);
    $d = 0;
    for ($j = $openIdx; $j < $n; $j++) {
        $t = $tokens[$j];
        if (is_string($t)) {
            if ($t === '{') $d++;
            elseif ($t === '}') { $d--; if ($d === 0) return $j; }
        } elseif (in_array($t[0], [T_CURLY_OPEN, T_DOLLAR_OPEN_CURLY_BRACES], true)) {
            $d++;
        }
    }
    throw new RuntimeException('unbalanced braces');
}

/**
 * Parse classlike members between body braces.
 * Member: kind (method|property|const|case), name (lowercase; methods/case
 * lowercase, props with $, consts original case), doc, span_start, span_end,
 * params (methods), names (all names if a multi-declaration).
 */
function parse_members(array $tokens, array $offs, int $bodyOpen, int $bodyClose, string $code): array
{
    $members = [];
    $j = $bodyOpen + 1;
    $pendingDocIdx = null; // docblock token awaiting its member
    while ($j < $bodyClose) {
        $t = $tokens[$j];
        if (is_array($t) && in_array($t[0], [T_WHITESPACE, T_COMMENT], true)) { $j++; continue; }
        if (is_array($t) && $t[0] === T_DOC_COMMENT) { $pendingDocIdx = $j; $j++; continue; }

        // use TraitName; inside class
        if (is_array($t) && $t[0] === T_USE) {
            while ($j < $bodyClose && !(is_string($tokens[$j]) && ($tokens[$j] === ';' || $tokens[$j] === '{'))) $j++;
            if (is_string($tokens[$j]) && $tokens[$j] === '{') $j = match_brace($tokens, $j);
            $j++;
            $pendingDocIdx = null;
            continue;
        }

        // Scan one member starting at $j (docblock, if any, was seen just before).
        $declStart = $pendingDocIdx ?? $j;
        $doc = $pendingDocIdx !== null ? $tokens[$pendingDocIdx][1] : null;
        $pendingDocIdx = null;
        $kind = null;
        $kwIdx = null;
        $k = $j;
        while ($k < $bodyClose) {
            $t = $tokens[$k];
            if (is_array($t)) {
                if ($t[0] === T_DOC_COMMENT) { $doc = $t[1]; $k++; continue; }
                if (in_array($t[0], [T_WHITESPACE, T_COMMENT], true)) { $k++; continue; }
                if (defined('T_ATTRIBUTE') && $t[0] === T_ATTRIBUTE) {
                    // skip to matching ]
                    $ad = 0;
                    while ($k < $bodyClose) {
                        $tt = $tokens[$k];
                        if (is_string($tt)) {
                            if ($tt === '[') $ad++;
                            elseif ($tt === ']') { $ad--; if ($ad === 0) { $k++; break; } }
                        } elseif (is_array($tt) && defined('T_ATTRIBUTE') && $tt[0] === T_ATTRIBUTE) {
                            $ad++; // T_ATTRIBUTE is '#['
                        }
                        $k++;
                    }
                    continue;
                }
                if (in_array($t[0], [T_PUBLIC, T_PROTECTED, T_PRIVATE, T_STATIC, T_ABSTRACT, T_FINAL, T_READONLY, T_VAR], true)) { $k++; continue; }
                if ($t[0] === T_FUNCTION) { $kind = 'method'; $kwIdx = $k; break; }
                if ($t[0] === T_CONST) { $kind = 'const'; $kwIdx = $k; break; }
                if (defined('T_ENUM_CASE') && $t[0] === T_ENUM_CASE) { $kind = 'case'; $kwIdx = $k; break; }
                if ($t[0] === T_CASE) { $kind = 'case'; $kwIdx = $k; break; }
                if ($t[0] === T_VARIABLE) { $kind = 'property'; $kwIdx = $k; break; }
                if ($t[0] === T_STRING && strtolower($t[1]) === 'case') { $kind = 'case'; $kwIdx = $k; break; }
                // type tokens before a property name (?int, array, Foo|Bar...)
                if (in_array($t[0], [T_STRING, T_NAME_QUALIFIED, T_NAME_FULLY_QUALIFIED, T_ARRAY, T_CALLABLE, T_NS_SEPARATOR], true)) { $k++; continue; }
                $k++;
                continue;
            } else {
                if (in_array($t, ['?', '|', '(', ')', '&'], true)) { $k++; continue; }
                if ($t === ';') { $kind = 'stray'; $kwIdx = $k; break; }
                $k++;
                continue;
            }
        }
        if ($kind === null || $kwIdx === null || $kwIdx >= $bodyClose) break;

        if ($kind === 'stray') {
            $j = $kwIdx + 1;
            continue;
        }

        if ($kind === 'method') {
            // name — methods may use semi-reserved keywords (list, array, print...),
            // which tokenize as their keyword token, not T_STRING.
            $nameIdx = null;
            for ($m = $kwIdx + 1; $m < $bodyClose; $m++) {
                $t = $tokens[$m];
                if (is_array($t) && in_array($t[0], [T_WHITESPACE, T_COMMENT, T_DOC_COMMENT], true)) continue;
                if (is_string($t) && $t === '&') continue;
                if (is_array($t) && preg_match('/^[A-Za-z_]\w*$/', $t[1])) $nameIdx = $m;
                break;
            }
            if ($nameIdx === null) { $j = $kwIdx + 1; continue; }
            [$params, $endOff] = function_tail($tokens, $offs, $nameIdx, $code);
            $members[] = [
                'kind' => 'method', 'name' => strtolower($tokens[$nameIdx][1]),
                'orig_name' => $tokens[$nameIdx][1],
                'doc' => $doc, 'span_start' => $offs[$declStart], 'span_end' => $endOff,
                'params' => $params,
            ];
            $j = token_at_offset($offs, count($tokens), $endOff) + 1;
            continue;
        }

        // const / case / property: ends at ';'
        $names = [];
        $endIdx = null;
        $pd = 0;
        for ($m = $kwIdx; $m < $bodyClose; $m++) {
            $t = $tokens[$m];
            if (is_string($t)) {
                if ($t === '(') $pd++;
                elseif ($t === ')') $pd--;
                elseif ($t === ';' && $pd === 0) { $endIdx = $m; break; }
            } elseif ($t[0] === T_VARIABLE && $kind === 'property') {
                $names[] = $t[1];
            } elseif ($t[0] === T_STRING && in_array($kind, ['const', 'case'], true) && empty($names)) {
                // first T_STRING after the keyword that isn't a type; consts are
                // `const [type] NAME = ...` — take the T_STRING immediately
                // preceding '='.
            }
        }
        if ($kind === 'const' || $kind === 'case') {
            // find name: T_STRING immediately before '=' (const) or before ';'/'=' (case)
            $prevStr = null;
            for ($m = $kwIdx + 1; $m <= ($endIdx ?? $bodyClose); $m++) {
                $t = $tokens[$m];
                if (is_string($t) && $t === '=') { break; }
                if (is_string($t) && $t === ';') break;
                if (is_array($t) && $t[0] === T_STRING) $prevStr = $t[1];
            }
            if ($prevStr !== null) $names[] = $prevStr;
        }
        if ($endIdx === null) break;
        $members[] = [
            'kind' => $kind,
            'name' => $kind === 'property' ? strtolower($names[0] ?? '?') : ($names[0] ?? '?'),
            'orig_name' => $names[0] ?? '?',
            'names' => $names,
            'doc' => $doc, 'span_start' => $offs[$declStart], 'span_end' => $offs[$endIdx] + 1,
            'params' => [],
        ];
        $j = $endIdx + 1;
    }
    return $members;
}

// ---------------------------------------------------------------------------
// Docblock merging
// ---------------------------------------------------------------------------

/**
 * Split docblock text into [description_lines[], entries[]] where each entry
 * is ['tag' => name, 'key' => group key, 'lines' => logical lines[]].
 */
function parse_docblock(?string $doc): array
{
    if ($doc === null) return [[], []];
    $inner = preg_replace('/^\/\*\*|\*\/$/s', '', trim($doc));
    $lines = preg_split('/\r?\n/', $inner);
    $logical = [];
    foreach ($lines as $line) {
        $l = preg_replace('/^\s*\*( ?)/', '', rtrim($line));
        $logical[] = $l;
    }
    // trim leading/trailing blank lines
    while ($logical && trim($logical[0]) === '') array_shift($logical);
    while ($logical && trim(end($logical)) === '') array_pop($logical);

    $desc = [];
    $entries = [];
    $cur = null;
    foreach ($logical as $l) {
        if (preg_match('/^@([a-zA-Z][\w-]*)/', ltrim($l), $m)) {
            if ($cur) $entries[] = $cur;
            $cur = ['tag' => strtolower($m[1]), 'lines' => [$l]];
        } elseif ($cur) {
            $cur['lines'][] = $l;
        } else {
            $desc[] = $l;
        }
    }
    if ($cur) $entries[] = $cur;

    foreach ($entries as &$e) {
        $e['key'] = tag_group_key($e['tag'], implode("\n", $e['lines']));
    }
    unset($e);
    return [$desc, $entries];
}

/** Group key: tags in the same group displace each other (Psalm wins). */
function tag_group_key(string $tag, string $text): string
{
    $paramTarget = '';
    if (preg_match('/(\$[A-Za-z_]\w*)/', $text, $m)) $paramTarget = strtolower($m[1]);

    return match (true) {
        in_array($tag, ['param', 'psalm-param', 'phpstan-param'], true) => "param:$paramTarget",
        in_array($tag, ['param-out', 'psalm-param-out'], true) => "param-out:$paramTarget",
        in_array($tag, ['return', 'psalm-return', 'phpstan-return'], true) => 'return',
        in_array($tag, ['var', 'psalm-var'], true) => "var:$paramTarget",
        in_array($tag, ['pure', 'psalm-pure'], true) => 'pure',
        in_array($tag, ['mutation-free', 'psalm-mutation-free'], true) => 'mutation-free',
        str_contains($tag, 'template') => 'template',  // whole-set group
        in_array($tag, ['implements', 'psalm-implements', 'template-implements'], true) => 'implements',
        in_array($tag, ['extends', 'psalm-extends', 'template-extends'], true) => 'extends',
        in_array($tag, ['method', 'psalm-method'], true) => "method:" . $text, // never displace distinct @method lines
        in_array($tag, ['property', 'psalm-property', 'property-read', 'property-write'], true) => "property:$paramTarget",
        default => "tag:$tag",
    };
}

/**
 * Merge a Psalm docblock onto an extension docblock.
 * Result: ext description, then Psalm content (desc+tags, param-renamed),
 * then retained ext-only entries. Returns docblock text or null if both empty.
 */
function merge_docblocks(?string $psalmDoc, ?string $extDoc, array $renames, string $indent): ?string
{
    if ($psalmDoc === null && $extDoc === null) return null;
    if ($psalmDoc !== null && $renames) {
        $psalmDoc = apply_renames($psalmDoc, $renames);
    }
    [$pDesc, $pEntries] = parse_docblock($psalmDoc);
    [$eDesc, $eEntries] = parse_docblock($extDoc);

    // Drop @php-from markers (informational only; reported by caller).
    $pEntries = array_values(array_filter($pEntries, fn($e) => $e['tag'] !== 'php-from'));

    $psalmKeys = [];
    $psalmHasTemplate = false;
    foreach ($pEntries as $e) {
        $psalmKeys[$e['key']] = true;
        if ($e['key'] === 'template') $psalmHasTemplate = true;
    }

    $kept = [];
    foreach ($eEntries as $e) {
        if (isset($psalmKeys[$e['key']])) continue;
        if ($psalmHasTemplate && in_array($e['key'], ['implements', 'extends'], true) && isset($psalmKeys[$e['key']])) continue;
        $kept[] = $e;
    }

    $blocks = [];
    if ($eDesc) $blocks[] = $eDesc;
    if ($pDesc) $blocks[] = $pDesc;
    $tagLines = [];
    foreach ($pEntries as $e) foreach ($e['lines'] as $l) $tagLines[] = $l;
    if ($kept && $tagLines) $tagLines[] = '';
    foreach ($kept as $e) foreach ($e['lines'] as $l) $tagLines[] = $l;
    if ($tagLines) $blocks[] = $tagLines;

    if (!$blocks) return null;

    $out = [];
    foreach ($blocks as $bi => $b) {
        if ($bi > 0) $out[] = '';
        foreach ($b as $l) $out[] = $l;
    }
    // collapse double blanks
    $final = [];
    $prevBlank = true;
    foreach ($out as $l) {
        $blank = trim($l) === '';
        if ($blank && $prevBlank) continue;
        $final[] = $l;
        $prevBlank = $blank;
    }
    while ($final && trim(end($final)) === '') array_pop($final);

    $txt = "/**\n";
    foreach ($final as $l) {
        $txt .= rtrim("$indent * $l") . "\n";
    }
    $txt .= "$indent */";
    return $txt;
}

/** Simultaneous $name renames using placeholders. */
function apply_renames(string $text, array $renames): string
{
    $placeholders = [];
    $i = 0;
    foreach ($renames as $from => $to) {
        $ph = "\x01R{$i}\x01";
        $text = preg_replace('/\\' . $from . '\b/', $ph, $text);
        $placeholders[$ph] = $to;
        $i++;
    }
    return strtr($text, $placeholders);
}

/** Positional param rename map between psalm decl and ext decl. */
function param_renames(array $psalmParams, array $extParams, string $sym): array
{
    $renames = [];
    if (count($psalmParams) !== count($extParams)) {
        report('WARN', "$sym: param count differs (psalm " . count($psalmParams) . " vs ext " . count($extParams) . ") — remapping by position up to min");
    }
    $min = min(count($psalmParams), count($extParams));
    for ($i = 0; $i < $min; $i++) {
        if (strtolower($psalmParams[$i]) !== strtolower($extParams[$i])) {
            $renames[$psalmParams[$i]] = $extParams[$i];
        }
    }
    if ($renames) {
        report('INFO', "$sym: renaming docblock params: " . json_encode($renames));
    }
    return $renames;
}

// ---------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------

class FileEditor
{
    public string $code;
    /** @var array<int, array{int,int,string}> */
    private array $edits = [];

    public function __construct(public string $path)
    {
        $this->code = file_exists($path) ? file_get_contents($path) : "<?php\n";
    }

    public function replace(int $start, int $end, string $text): void
    {
        $this->edits[] = [$start, $end, $text];
    }

    public function insert(int $at, string $text): void
    {
        $this->edits[] = [$at, $at, $text];
    }

    public function apply(): string
    {
        usort($this->edits, fn($a, $b) => $b[0] <=> $a[0]);
        $code = $this->code;
        $prevStart = PHP_INT_MAX;
        foreach ($this->edits as [$s, $e, $t]) {
            if ($e > $prevStart) throw new RuntimeException("overlapping edits in {$this->path}");
            $code = substr($code, 0, $s) . $t . substr($code, $e);
            $prevStart = $s;
        }
        $this->edits = [];
        return $code;
    }
}

/** Indentation of the line containing byte offset. */
function indent_at(string $code, int $off): string
{
    $lineStart = strrpos(substr($code, 0, $off), "\n");
    $lineStart = $lineStart === false ? 0 : $lineStart + 1;
    preg_match('/^[ \t]*/', substr($code, $lineStart, $off - $lineStart), $m);
    return $m[0];
}

/** Docblock span within a decl: [start, end) of the doc text inside the span, or null. */
function doc_span(string $code, array $decl): ?array
{
    if ($decl['doc'] === null) return null;
    $pos = strpos($code, $decl['doc'], $decl['span_start']);
    if ($pos === false || $pos >= $decl['span_end']) return null;
    return [$pos, $pos + strlen($decl['doc'])];
}

// ---------------------------------------------------------------------------
// Grafting
// ---------------------------------------------------------------------------

/**
 * Graft one source decl onto the target file's matching decl.
 * Returns true if handled (target decl found or appended).
 */
function graft_decl(array $src, FileEditor $tgt, array $tgtDecls, string $relSrc, ?string $since): bool
{
    $matches = array_values(array_filter($tgtDecls, fn($d) => $d['fqn'] === $src['fqn']));
    if (!$matches) {
        // Append whole decl (relocation case).
        $text = substr_with_doc($src);
        if ($since !== null) {
            $text = add_since($text, $src, $since);
        }
        $tgt->insert(strlen($tgt->code), "\n" . $text . "\n");
        report('INFO', "{$src['fqn']}: appended whole declaration from $relSrc to " . basename($tgt->path));
        return true;
    }
    if (count($matches) > 1) {
        report('WARN', "{$src['fqn']}: target " . basename($tgt->path) . " declares it " . count($matches) . " times — grafting onto the first");
    }
    $t = $matches[0];

    if ($src['kind'] === 'function') {
        if ($t['kind'] !== 'function') {
            report('ERROR', "{$src['fqn']}: kind mismatch ({$src['kind']} vs {$t['kind']})");
            return false;
        }
        graft_doc($src, $t, $tgt, $src['fqn']);
        return true;
    }

    // Classlike
    if ($t['kind'] !== $src['kind']) {
        report('WARN', "{$src['fqn']}: classlike kind differs (psalm {$src['kind']} vs ext {$t['kind']}) — grafting docblocks anyway");
    }
    // Class-level docblock.
    graft_doc($src, $t, $tgt, $src['fqn'], isClasslike: true);

    // Compare implements/extends lists for reporting.
    compare_heads($src, $t);

    // Members.
    $tgtMembersByKey = [];
    foreach ($t['members'] as $m) {
        $tgtMembersByKey[$m['kind'] . ':' . strtolower($m['name'])] = $m;
    }
    $appendBuf = '';
    foreach ($src['members'] as $sm) {
        $key = $sm['kind'] . ':' . strtolower($sm['name']);
        if (isset($tgtMembersByKey[$key])) {
            graft_doc($sm, $tgtMembersByKey[$key], $tgt, "{$src['fqn']}::{$sm['orig_name']}");
        } else {
            $memberIndent = '    ';
            $text = substr_with_doc($sm, sourceCode: null);
            $appendBuf .= "\n" . reindent($text, $memberIndent) . "\n";
            report('INFO', "{$src['fqn']}::{$sm['orig_name']} ({$sm['kind']}): not in ext — appending Psalm declaration");
        }
    }
    if ($appendBuf !== '') {
        $tgt->insert($t['body_close'], $appendBuf);
    }
    return true;
}

/** The source text of a decl/member including its docblock. */
function substr_with_doc(array $decl, ?string $sourceCode = null): string
{
    return $decl['__source_text'];
}

function reindent(string $text, string $indent): string
{
    $lines = explode("\n", $text);
    // The first line starts at the extraction offset and so carries no leading
    // whitespace; compute the minimal indent over the REMAINING non-blank
    // lines, strip it from them, and indent everything uniformly.
    $min = null;
    foreach (array_slice($lines, 1) as $l) {
        if (trim($l) === '') continue;
        preg_match('/^[ \t]*/', $l, $m);
        $cur = strlen($m[0]);
        $min = $min === null ? $cur : min($min, $cur);
    }
    $min = $min ?? 0;
    $out = [];
    foreach ($lines as $i => $l) {
        if (trim($l) === '') { $out[] = ''; continue; }
        $out[] = $i === 0 ? $indent . ltrim($l) : $indent . substr($l, $min);
    }
    return implode("\n", $out);
}

function add_since(string $declText, array $src, string $since): string
{
    $tag = "@since $since PHP";
    if ($src['doc'] !== null && str_contains($declText, $src['doc'])) {
        // splice into existing docblock before closing */
        $newDoc = preg_replace('/\n(\s*)\*\/\s*$/', "\n\$1* $tag\n\$1*/", $src['doc'], 1);
        if ($newDoc !== null && $newDoc !== $src['doc']) {
            return str_replace($src['doc'], $newDoc, $declText);
        }
    }
    return "/**\n * $tag\n */\n" . $declText;
}

/** Merge src docblock into target decl's docblock and queue the edit. */
function graft_doc(array $src, array $tgtDecl, FileEditor $tgt, string $symLabel, bool $isClasslike = false): void
{
    if ($src['doc'] === null) return; // nothing to graft

    $renames = [];
    if (!$isClasslike && (!empty($src['params']) || !empty($tgtDecl['params']))) {
        $renames = param_renames($src['params'] ?? [], $tgtDecl['params'] ?? [], $symLabel);
    }

    if (str_contains($src['doc'], '@php-from')) {
        report('WARN', "$symLabel: source docblock contains @php-from marker (dropped) — review manually");
    }

    $span = doc_span($tgt->code, $tgtDecl);
    $indent = indent_at($tgt->code, $tgtDecl['span_start']);
    $merged = merge_docblocks($src['doc'], $tgtDecl['doc'], $renames, $indent);
    if ($merged === null) return;

    if ($span !== null) {
        $tgt->replace($span[0], $span[1], $merged);
    } else {
        $tgt->insert($tgtDecl['span_start'], $merged . "\n" . $indent);
    }
}

function compare_heads(array $src, array $tgt): void
{
    $norm = function (?string $head): array {
        if ($head === null) return [[], []];
        $head = preg_replace('/\/\*.*?\*\//s', '', $head);
        $ext = [];
        $impl = [];
        if (preg_match('/\bextends\s+(.+?)(\bimplements\b|$)/s', $head, $m)) {
            $ext = array_map('trim', explode(',', trim($m[1])));
        }
        if (preg_match('/\bimplements\s+(.+)$/s', $head, $m)) {
            $impl = array_map('trim', explode(',', trim($m[1])));
        }
        return [array_map('strtolower', array_filter($ext)), array_map('strtolower', array_filter($impl))];
    };
    [$sExt, $sImpl] = $norm($src['head'] ?? null);
    [$tExt, $tImpl] = $norm($tgt['head'] ?? null);
    $missingImpl = array_diff($sImpl, $tImpl);
    $missingExt = array_diff($sExt, $tExt);
    if ($missingImpl) {
        report('WARN', "{$src['fqn']}: psalm implements [" . implode(', ', $missingImpl) . "] not in ext head — review (ext head kept)");
    }
    if ($missingExt) {
        report('WARN', "{$src['fqn']}: psalm extends [" . implode(', ', $missingExt) . "] not in ext head — review (ext head kept)");
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

// Build symbol → extension-file index.
$extFiles = glob(STUBS . '/extensions/*.phpstub');
$extDeclIndex = []; // fqn => relpath
foreach ($extFiles as $f) {
    foreach (parse_decls(file_get_contents($f)) as $d) {
        $extDeclIndex[$d['fqn']][] = basename($f);
    }
}

$editors = []; // path => FileEditor
function editor(string $path): FileEditor
{
    global $editors;
    if (!isset($editors[$path])) {
        if (!file_exists($path)) {
            report('INFO', 'creating new stub file ' . basename($path));
        }
        $editors[$path] = new FileEditor($path);
    }
    return $editors[$path];
}

$deletions = []; // root files to delete entirely

foreach ($ROOT_ORDER as $rootName) {
    if ($ONLY !== null && $rootName !== $ONLY) continue;
    $rootPath = STUBS . '/' . $rootName;
    if (!file_exists($rootPath)) { report('WARN', "missing root file $rootName"); continue; }
    $code = file_get_contents($rootPath);
    $decls = parse_decls($code);
    // Attach raw source text for appends.
    foreach ($decls as &$d) {
        $d['__source_text'] = substr($code, $d['span_start'], $d['span_end'] - $d['span_start']);
        foreach ($d['members'] ?? [] as $mi => $m) {
            $d['members'][$mi]['__source_text'] = substr($code, $m['span_start'], $m['span_end'] - $m['span_start']);
        }
    }
    unset($d);

    $srcEditor = new FileEditor($rootPath);
    $migrated = 0;

    foreach ($decls as $d) {
        global $RELOCATIONS, $extDeclIndex;
        $since = null;
        $targets = $extDeclIndex[$d['fqn']] ?? [];
        $targets = array_values(array_unique($targets));
        if (count($targets) > 1) {
            report('ERROR', "{$d['fqn']}: multiple extension files declare it (" . implode(', ', $targets) . ") — skipping, resolve first");
            continue;
        }
        if (!$targets) {
            if (!isset($RELOCATIONS[$d['fqn']])) {
                report('WARN', "{$d['fqn']} ($rootName): no extension target and no relocation rule — left in place");
                continue;
            }
            [$tf, $since] = $RELOCATIONS[$d['fqn']];
            $targetPath = STUBS . '/extensions/' . $tf;
        } else {
            $targetPath = STUBS . '/extensions/' . $targets[0];
        }

        $tgt = editor($targetPath);
        $tgtDecls = parse_decls($tgt->code);
        if (graft_decl($d, $tgt, $tgtDecls, $rootName, $since)) {
            // Apply target edits eagerly so subsequent decls parse fresh state.
            $newCode = $tgt->apply();
            $tgt->code = $newCode;
            file_put_contents_dry($targetPath, $newCode);
            // Remove from source (span + trailing blank line).
            $end = $d['span_end'];
            while ($end < strlen($code) && in_array($code[$end], ["\n", "\r"], true)) $end++;
            $srcEditor->replace($d['span_start'], $end, '');
            $migrated++;
        }
    }

    $newRoot = $srcEditor->apply();
    if (is_effectively_empty($newRoot)) {
        $deletions[] = $rootPath;
        report('INFO', "$rootName: fully migrated ($migrated decls) — deleting file");
        if (!$GLOBALS['DRY_RUN']) unlink($rootPath);
    } else {
        report('INFO', "$rootName: migrated $migrated decls, residue remains — review " . basename($rootPath));
        file_put_contents_dry($rootPath, $newRoot);
    }
}

function file_put_contents_dry(string $path, string $content): void
{
    if ($GLOBALS['DRY_RUN']) return;
    file_put_contents($path, $content);
}

/** True if the file holds no declarations: only comments, namespace shells, whitespace. */
function is_effectively_empty(string $code): bool
{
    foreach (token_get_all($code) as $t) {
        if (is_string($t)) {
            if (in_array($t, ['{', '}', ';'], true)) continue;
            return false;
        }
        if (in_array($t[0], [T_OPEN_TAG, T_WHITESPACE, T_COMMENT, T_DOC_COMMENT,
            T_NAMESPACE, T_STRING, T_NAME_QUALIFIED], true)) continue;
        return false;
    }
    return true;
}

$warnings = count(array_filter($REPORT, fn($r) => $r[0] === 'WARN'));
$errors = count(array_filter($REPORT, fn($r) => $r[0] === 'ERROR'));
fwrite(STDERR, "\nDone. $warnings warnings, $errors errors" . ($DRY_RUN ? ' (dry run)' : '') . "\n");
exit($errors > 0 ? 1 : 0);
