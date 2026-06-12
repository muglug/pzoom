#!/usr/bin/env python3
"""Psalm⇆pzoom parity score: how faithfully does pzoom (Rust) mirror Psalm (PHP)?

Unlike the broad ``similarity_heuristic.py`` (file-level token cosine over three
projects), this tool is Psalm-only and works at *method* granularity. The pzoom
side is the ``pzoom-analyzer`` and ``pzoom-code-info`` crates only:

  * For every Psalm file that is in pzoom's scope it finds the pzoom
    counterpart from PSALM_FILE_MAP.json — a complete map of every file in
    the two crates to its Psalm counterpart (null = pzoom-specific; a list =
    one pzoom file covering several small Psalm files).
  * Within a matched file pair it lines up similarly-named methods and compares
    *which members / methods / functions they reference*, and how often.
  * Names are canonicalised so cross-language renames line up:
      - snake_case / PascalCase / camelCase  ->  same word tokens
      - ``FunctionLikeStorage`` (Psalm) == ``FunctionLikeInfo`` (pzoom)   [*Storage→Info rule*]
      - ``$context->vars_in_scope`` == ``context.locals``                [*synonym map*]
      - Psalm getter ``getCodebase()`` == pzoom field ``codebase``       [*get- strip rule*]
  * Interning and reporting plumbing (issue buffer / location objects) are
    framework-specific and ignored; PHP↔Rust idiom is folded (TUnion→Union,
    get_expr_type→getType, …) so renames don't look like logic gaps. Tokens that
    never occur in Psalm carry zero weight, so pzoom isn't penalised for them.
  * The per-file score is **recall × precision**: recall = how much of Psalm's
    logic pzoom mirrors; precision = 1 − the share of pzoom's logic that is
    *introduced* (functions the Psalm file has under no naming — see
    ``find_introduced``).
  * **A Psalm file with no pzoom equivalent scores 0** and is weighted by its
    reference mass — i.e. a big missing file hurts the project score a lot.

Output: a Markdown report (the project parity score, the biggest missing
files, and a per-file / per-method breakdown). Pure standard library.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path

# --------------------------------------------------------------------------- #
# Canonicalisation: identifier  ->  cross-language concept token
# --------------------------------------------------------------------------- #

_WORD_RE = re.compile(r"[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|\d+")

# If an identifier contains any of these words it is framework plumbing with no
# cross-language analog and is dropped wholesale (see --keep-interning).
#  - interning: Rust-specific string-interning optimisation (interner / StrId).
#  - rust containers: Rc/RefCell/FxHashMap … impl detail, never in Psalm anyway.
#  - psalm impl: the immutable-Union builder/freeze mechanism and the PhpParser
#    AST library are Psalm-side impl details with no pzoom analog — the PHP
#    mirror of interning, so we ignore them symmetrically.
INTERNING_WORDS = frozenset("intern interner interned strid".split())
PLUMBING_WORDS = frozenset(
    "fx rc arc refcell rwlock mutex cow builder freeze".split())

# Canonical tokens dropped after normalisation. All are framework plumbing with
# no cross-language analog — the PHP mirror of Rust interning — so ignoring them
# keeps both scoring and the "missing construct" hints meaningful. (Handled here,
# not in PLUMBING_WORDS, because e.g. "PhpParser" splits into php+parser and
# never appears as a single plumbing word.)
#  - namespace / AST: PHP namespace path tokens + the PhpParser AST library
#    (pzoom uses the mago parser).
#  - reporting / location idiom: Psalm reports issues via IssueBuffer + a
#    CodeLocation object and threads suppression lists, where pzoom carries a
#    byte span and emits an IssueKind directly. These tokens are pervasive in
#    every Psalm analyzer yet have ~no pzoom counterpart, so they otherwise
#    drown out real logic differences. (The *act* of emitting an issue is kept:
#    maybeAdd folds to add_issue via SYNONYMS.)
NOISE_CANON = frozenset(
    "psalm internal phpparser parser node nodeabstract virtual "
    "codelocation issuebuffer source".split())

# Any canonical token containing one of these is reporting bookkeeping idiom
# (issue *suppression* lists: getSuppressedIssues / addSuppressedIssues /
# removeSuppressedIssues / suppressed_issues …) and is dropped.
NOISE_SUBSTRINGS = ("suppressed",)

# Rust stdlib idioms with no cross-language analog: Option/Result combinators,
# iterator/container methods and string helpers. These are the Rust mirror of
# interning — pure plumbing the PHP side never spells out (PHP has no `Some`,
# `unwrap_or`, `is_empty`, `and_then`, …) — so they otherwise sit in every
# pzoom file as un-Psalm-able noise, drowning real logic and depressing the
# grounding signal. Dropped symmetrically (only behavioural intent survives:
# e.g. `is_nothing`/`nothing` = the Nothing type, kept; `contains_key`/`keys`
# vs PHP array spelling are deliberately NOT here to avoid eating domain signal).
# `strid` belongs to interning but splits to str+id, so the INTERNING_WORDS
# entry never fires — caught here at the canonical-token level instead.
# Tokens Psalm *also* uses (tostring/trim/collect/clone/none) are deliberately
# excluded: they're shared vocabulary (PHP trim()/clone/__toString ↔ the Rust
# equivalents), so dropping them would erase real matches and depress recall.
RUST_IDIOM = frozenset(
    """
    some isnone issome isok iserr issomeand isnoneor isokand mapor
    maporelse unwrap unwrapor unwrapordefault unwraporelse unwrapunchecked
    okor okorelse andthen ortelse getorinsert getorinsertwith ordefault
    takeif expect ok err iter itermut intoiter copied cloned
    enumerate extend push pushstr pop len isempty containskey entry drain
    retain dedup truncate withcapacity toowned asstr asref asmut
    asslice startswith endswith stripprefix stripsuffix trimstartmatches
    trimendmatches trimstart trimend eqignoreasciicase toasciilowercase
    toasciiuppercase tolowercase touppercase chars bytes saturatingadd
    saturatingsub saturatingmul checkedadd deref derefmut borrow
    borrowmut strid
    """.split()
)

# Ubiquitous syntax / structural words with no domain signal. (Deliberately
# excludes isset/unset/foreach etc. that appear inside meaningful compounds like
# inside_isset; only standalone control words that don't form compounds.)
KEYWORDS = frozenset(
    """
    fn let pub mut impl struct enum trait mod const static use crate super
    self this return new function public private protected final abstract
    namespace class interface extends implements echo void null true false
    print var match if else elseif for foreach endif endforeach endwhile
    endswitch while loop where dyn box move async await as instanceof goto declare
    """.split()
)

# Irregular cross-language renames that the rules below can't derive.
# Keyed by the *concatenated normalised* form; value is the shared canonical.
# (Storage→Info and get-stripping are handled by rule, not here.)
SYNONYMS = {
    "varsinscope": "locals",                  # Psalm Context->vars_in_scope == pzoom locals
    "varspossiblyinscope": "possiblylocals",
    "parent": "parentclass",                  # Psalm Context->parent  == pzoom parent_class
    "dim": "offset",                          # Psalm array $dim       == pzoom offset
    "nodedata": "analysisdata",               # Psalm node_data        == pzoom analysis_data
    "fqcln": "classname",                     # Psalm $fq_class_name abbreviations
    "fqclassname": "classname",
    "fqclasslikename": "classname",
    "getclasslikestorage": "classlikeinfo",   # Codebase->getClassLikeStorage()
    "getfunctionlikestorage": "functionlikeinfo",
    "getmethodstorage": "methodinfo",
    # --- PHP↔Rust idiom folds: same concept, different spelling. Folding these
    #     stops cross-language renames from masquerading as real logic gaps, so
    #     genuine behavioural differences (e.g. Traversable handling) stand out.
    "tunion": "union",                        # pzoom TUnion        == Psalm Type\Union
    "tatomic": "atomic",                      # pzoom TAtomic       == Psalm Type\Atomic
    "exprtype": "type",                       # pzoom get_expr_type == Psalm node_data->getType
    "setexprtype": "settype",                 # pzoom set_expr_type == Psalm node_data->setType
    "maybeadd": "addissue",                   # Psalm IssueBuffer::maybeAdd == pzoom add_issue
    "maybeaddissue": "addissue",
}


def canon(ident: str, keep_interning: bool) -> str | None:
    """Reduce one identifier to a cross-language canonical concept token."""
    words = [w.lower() for w in _WORD_RE.findall(ident)]
    if not words:
        return None
    if not keep_interning and any(w in INTERNING_WORDS or w in PLUMBING_WORDS
                                  for w in words):
        return None
    words = [w for w in words if w not in KEYWORDS]
    if not words:
        return None
    if words[-1] == "storage":                # FunctionLikeStorage -> ...info
        words[-1] = "info"
    if len(words) > 1 and words[0] in ("get",):   # getCodebase() -> codebase
        words = words[1:]
    key = "".join(words)
    key = SYNONYMS.get(key, key)
    if not keep_interning and (key in NOISE_CANON or key in RUST_IDIOM
                               or any(s in key for s in NOISE_SUBSTRINGS)):
        return None
    return key if len(key) >= 2 else None


# --------------------------------------------------------------------------- #
# Source stripping + reference / method extraction
# --------------------------------------------------------------------------- #

def strip_code(text: str, lang: str = "php") -> str:
    """Blank out string literals and comments so only code identifiers remain.

    Single left-to-right pass with explicit state (a regex per literal kind
    spans across code when quotes/apostrophes are unbalanced — e.g. an
    apostrophe in a comment — which silently eats function signatures). Output
    preserves length and newlines, so byte offsets stay valid."""
    is_rust = lang == "rust"
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        two = text[i:i + 2]
        if two == "//" or (not is_rust and c == "#"):           # line comment
            while i < n and text[i] != "\n":
                out.append(" "); i += 1
        elif two == "/*":                                       # block comment
            while i < n and text[i:i + 2] != "*/":
                out.append("\n" if text[i] == "\n" else " "); i += 1
            for _ in range(2):
                if i < n:
                    out.append(" "); i += 1
        elif c in '"`' or (c == "'" and not is_rust):           # string literal
            quote = c
            out.append(" "); i += 1
            while i < n and text[i] != quote:
                if text[i] == "\\" and i + 1 < n:
                    out.append("  "); i += 2
                else:
                    out.append("\n" if text[i] == "\n" else " "); i += 1
            if i < n:
                out.append(" "); i += 1
        elif c == "'" and is_rust:                              # rust char vs lifetime
            if text[i:i + 2] == "'\\":                          # '\n' etc.
                j = text.find("'", i + 2)
                if 0 <= j <= i + 4:
                    out.append(" " * (j - i + 1)); i = j + 1
                else:
                    out.append(c); i += 1
            elif i + 2 < n and text[i + 2] == "'":              # 'x'
                out.append("   "); i += 3
            else:                                               # lifetime / label
                out.append(c); i += 1
        else:
            out.append(c); i += 1
    return "".join(out)


_PHP_FN = re.compile(r"\bfunction\s+([A-Za-z_]\w*)\s*\(")
_RUST_FN = re.compile(r"\bfn\s+([A-Za-z_]\w*)\s*[(<]")

_REF_MEMBER = re.compile(r"(?:->|::|\.)\s*([A-Za-z_]\w*)")
_REF_CALL = re.compile(r"\b([A-Za-z_]\w*)\s*\(")
_REF_TYPE = re.compile(r"\b([A-Z][A-Za-z0-9_]*)\b")


@dataclass
class Method:
    name: str            # canonical name
    raw_name: str
    refs: Counter        # canonical reference token -> count


def extract_refs(code: str, keep_interning: bool, examples: dict | None = None) -> Counter:
    """Count canonical construct references (members, calls, types, and via the
    member regex also field/property declarations) in a slice of stripped code.
    If ``examples`` is given, record a readable raw spelling per canonical token
    (preferring the longest seen) so reports can name the construct."""
    refs: Counter = Counter()
    for rx in (_REF_MEMBER, _REF_CALL, _REF_TYPE):
        for ident in rx.findall(code):
            c = canon(ident, keep_interning)
            if c:
                refs[c] += 1
                if examples is not None and len(ident) > len(examples.get(c, "")):
                    examples[c] = ident
    return refs


# Field / property declarations, so data-holder classes (Storage/Info, scopes,
# result structs) are compared on their members, not just method bodies.
_PHP_PROP = re.compile(r"(?:public|protected|private|readonly)\s+[^;{]*?\$([A-Za-z_]\w*)")
_RUST_FIELD = re.compile(r"(?:pub(?:\([^)]*\))?\s+)?([a-z_]\w*)\s*:\s*[A-Za-z_&<']")


def extract_methods(text: str, lang: str,
                    keep_interning: bool) -> tuple[list[Method], Counter, dict]:
    """Return (per-method reference counts, whole-file reference bag, examples).
    The bag spans the entire file (incl. field/property/type declarations); the
    method list drives the per-method structural comparison."""
    code = strip_code(text, lang)
    examples: dict[str, str] = {}
    bag = extract_refs(code, keep_interning, examples)
    prop_re = _PHP_PROP if lang == "php" else _RUST_FIELD
    for fld in prop_re.findall(code):
        c = canon(fld, keep_interning)
        if c:
            bag[c] += 1
            if len(fld) > len(examples.get(c, "")):
                examples[c] = fld
    fn_re = _PHP_FN if lang == "php" else _RUST_FN
    defs = [(m.start(), m.group(1)) for m in fn_re.finditer(code)]
    methods: list[Method] = []
    for i, (start, raw) in enumerate(defs):
        end = defs[i + 1][0] if i + 1 < len(defs) else len(code)
        refs = extract_refs(code[start:end], keep_interning)
        cname = canon(raw, keep_interning) or raw.lower()
        methods.append(Method(name=cname, raw_name=raw, refs=refs))
    return methods, bag, examples


# --------------------------------------------------------------------------- #
# Documents
# --------------------------------------------------------------------------- #


@dataclass
class Doc:
    path: Path
    rel: str
    methods: list[Method]
    bag: Counter                              # whole-file reference bag
    examples: dict                            # canonical token -> readable raw


def load_docs(root: Path, ext: str, lang: str, keep_interning: bool) -> dict[str, Doc]:
    docs = {}
    for path in sorted(root.rglob(f"*{ext}")):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        rel = str(path.relative_to(root))
        methods, bag, examples = extract_methods(text, lang, keep_interning)
        docs[rel] = Doc(path=path, rel=rel, methods=methods, bag=bag, examples=examples)
    return docs


# --------------------------------------------------------------------------- #
# Mapping (PSALM_FILE_MAP.json):  pzoom rel path -> Psalm rel path(s) | null
# --------------------------------------------------------------------------- #

# The pzoom side of the score: only these crates mirror Psalm file-for-file.
SCOPE_CRATES = ("pzoom-analyzer", "pzoom-code-info")

# Psalm files ported in pzoom crates *outside* the scope crates (pzoom-syntax,
# pzoom-orchestrator, pzoom-cli). They are excluded from the score entirely:
# neither matched (their port isn't in scope) nor missing (they aren't gaps).
PORTED_OUTSIDE_SCOPE = frozenset({
    # pzoom-orchestrator
    "Internal/Analyzer/ProjectAnalyzer.php",
    "Internal/Codebase/Analyzer.php",
    "Internal/Codebase/Scanner.php",
    "Internal/Codebase/Populator.php",
    "Internal/Cache.php",
    "Internal/Diff/AstDiffer.php",
    # pzoom-syntax (docblock / declaration scanning)
    "Internal/Scanner/ParsedDocblock.php",
    "Internal/Type/TypeParser.php",
    "Internal/Type/TypeTokenizer.php",
    "Internal/Type/ParseTree.php",
    "Internal/Type/ParseTreeCreator.php",
    "Internal/Analyzer/Statements/Expression/SimpleTypeInferer.php",
    # pzoom-cli (issue collection / report rendering)
    "IssueBuffer.php",
    "Report.php",
})


def load_file_map(path: Path) -> dict[str, list[str]]:
    """Read the complete pzoom→Psalm file map and return the inverse:
    Psalm rel path (under src/Psalm) -> [pzoom repo-relative paths].

    The JSON is a dict over *every* ``.rs`` file in the scope crates; a value is
    a Psalm path, a list of Psalm paths (one pzoom file covering several small
    Psalm files), or null (pzoom-specific — module roots, Hakana ports,
    conscious departures). Keyed by full paths, so duplicate Psalm basenames
    can't mis-bind, and there is no fuzzy filename fallback."""
    inv: dict[str, list[str]] = defaultdict(list)
    data = json.loads(path.read_text(encoding="utf-8"))
    for pz, psalm in data.items():
        if not psalm:
            continue
        for p in (psalm if isinstance(psalm, list) else [psalm]):
            inv[p].append(pz)
    return inv


# --------------------------------------------------------------------------- #
# Scoring
# --------------------------------------------------------------------------- #


def build_weights(docs: dict[str, Doc], mode: str) -> dict[str, float]:
    """Per-token weight, keyed only by tokens that occur in the Psalm corpus
    (so Rust-only plumbing on the pzoom side gets weight 0 via ``.get(t, 0)``).

    mode='binary' -> every Psalm-vocabulary token weighs 1 (frequency still
    matters through the multiset Dice). mode='idf' -> rarer constructs weigh
    more (good for surfacing distinctive logic, harsher on cross-language
    structural divergence)."""
    df: Counter = Counter()
    n = 0
    for d in docs.values():
        for m in d.methods:
            n += 1
            df.update(m.refs.keys())
    if mode == "idf":
        return {t: math.log((n + 1) / (c + 1)) + 1.0 for t, c in df.items()}
    return {t: 1.0 for t in df}


def w_mass(refs: Counter, idf: dict[str, float]) -> float:
    """Total Psalm-IDF-weighted reference mass (tokens unknown to Psalm = 0)."""
    return sum(idf.get(t, 0.0) * c for t, c in refs.items())


def weighted_recall(psalm: Counter, pz: Counter, idf: dict[str, float]) -> float:
    """Directional: of Psalm's weighted construct references, what fraction is
    mirrored on the pzoom side? Tokens absent from the Psalm corpus carry zero
    weight, so Rust-only plumbing neither helps nor hurts. This does not punish
    pzoom for referencing *more* than Psalm (extra detail, helper splits)."""
    inter = sum(idf.get(t, 0.0) * min(c, pz.get(t, 0)) for t, c in psalm.items())
    pm = w_mass(psalm, idf)
    return 0.0 if pm == 0 else inter / pm


def weighted_precision(psalm: Counter, pz: Counter, idf: dict[str, float]) -> float:
    """Directional mirror of :func:`weighted_recall`, normalised by the *pzoom*
    side: of the Psalm-vocabulary concepts pzoom's file references, what fraction
    does the matched Psalm file also reference?

    Diagnostic only — never folded into the headline. Because tokens absent from
    the Psalm corpus carry zero IDF, this excludes both Rust-only plumbing and
    pzoom's divergent type-atom vocabulary; what it measures is *relocation* — a
    low value flags a pzoom file that pulls in Psalm concepts living in *other*
    Psalm files (logic Psalm spread across files, or pzoom co-located). Pair it
    with the novel share (:func:`pz_novel_share`) for the pzoom-only signal."""
    pm = w_mass(pz, idf)
    if pm == 0:
        return 1.0
    inter = sum(idf.get(t, 0.0) * min(c, psalm.get(t, 0)) for t, c in pz.items())
    return inter / pm


def pz_novel_share(pz: Counter, vocab: set) -> float:
    """Diagnostic: share of pzoom's canonical references absent from Psalm
    entirely — pzoom-only vocabulary or behaviour (after plumbing is stripped,
    mostly divergent type atoms + genuinely introduced logic)."""
    tot = sum(pz.values())
    if not tot:
        return 0.0
    return sum(c for t, c in pz.items() if t not in vocab) / tot


def name_match(pm: Method, candidates: list[Method]) -> Method | None:
    """Find the pzoom method whose canonical name matches a Psalm method."""
    exact = [c for c in candidates if c.name == pm.name]
    if exact:
        return exact[0] if len(exact) == 1 else max(
            exact, key=lambda c: sum((c.refs & pm.refs).values()))
    # fall back to high word-token overlap of the names
    pw = set(_WORD_RE.findall(pm.raw_name.lower()))
    best, best_j = None, 0.0
    for c in candidates:
        cw = set(_WORD_RE.findall(c.raw_name.lower()))
        if not pw or not cw:
            continue
        j = len(pw & cw) / len(pw | cw)
        if j > best_j:
            best, best_j = c, j
    return best if best_j >= 0.6 else None


# Rust-idiom methods (constructors, trait impls, formatting) — required by the
# language, not behaviour Psalm "lacks", so they're never counted as introduced.
RUST_IDIOM_METHODS = frozenset(
    "new default fmt clone eq ne hash cmp partialcmp partialeq from tryfrom into "
    "asref asmut asstr deref derefmut drop next tostring toowned borrow "
    "withcapacity".split())


RELOCATION_THRESHOLD = 0.5
# Tokens appearing in more than this share of corpus files prove nothing about
# where a function's behaviour lives — generated kitchen-sink files (e.g.
# PreloaderList.php) would otherwise blanket-exonerate everything.
RELOCATION_MAX_DF = 0.25


def find_introduced(pz_methods: list[Method], psalm: Doc, vocab: set,
                    min_size: int, threshold: float,
                    relocation_corpora: list[tuple[str, set]] | None = None,
                    relocation_df: dict[str, float] | None = None,
                    ) -> tuple[float, list, list]:
    """Detect pzoom functions the corresponding Psalm file has under *no* naming.

    A pzoom function is "introduced" when it is **both** differently-named from
    every Psalm method **and** poorly *grounded* — most of the *domain* concepts
    it references never appear in the Psalm file. Grounding is what guards
    against false positives from renames/decomposition: a renamed or split-out
    helper still references Psalm's types/methods/concepts, so it grounds high
    and is not flagged. Only genuinely new behaviour is.

    Grounding is computed over *domain* tokens only — those that occur somewhere
    in the Psalm corpus (`vocab`) — so pzoom-local identifiers (helper names,
    private vars) don't drag a faithful helper's grounding down. A function that
    references *nothing* Psalm knows is treated as ungrounded (fully novel).

    The penalty is weighted by each function's **size** (its total reference
    count, a proxy for how much code it is), not just its Psalm-domain mass — so
    a large pzoom-specific method counts for much more than a small one, and
    removing one visibly raises parity. precision = 1 − (introduced size / all
    pzoom size).

    A candidate is *exonerated* (reported as relocated, not introduced) when it
    grounds strongly (≥ RELOCATION_THRESHOLD) in some single **other** Psalm
    file or in a Hakana file: pzoom legitimately consolidates logic from
    several Psalm files (e.g. FunctionLikeNodeScanner checks living beside
    FunctionAnalyzer code) and ports dataflow machinery from Hakana per
    AGENTS.md — neither is behaviour Psalm lacks."""
    psalm_tokens = set(psalm.bag)
    psalm_names = {m.name for m in psalm.methods}
    total = introduced_mass = 0
    rows = []
    relocated_rows = []
    for f in pz_methods:
        size = sum(f.refs.values())                    # ≈ method size (all refs)
        if size == 0:
            continue
        if f.name in RUST_IDIOM_METHODS:               # language plumbing, skip
            continue
        total += size
        if f.name in psalm_names:                      # same concept, kept name
            continue
        domain = {t: c for t, c in f.refs.items() if t in vocab}
        dsize = sum(domain.values())
        grounded = sum(c for t, c in domain.items() if t in psalm_tokens)
        grounding = grounded / dsize if dsize else 0.0  # no domain refs ⇒ novel
        if size >= min_size and grounding < threshold:
            relocation_home = None
            if relocation_corpora:
                # Ground only on *discriminative* tokens: drop those present in
                # most corpus files before asking which single file knows them.
                discr = {
                    t: c for t, c in domain.items()
                    if relocation_df is None
                    or relocation_df.get(t, 1.0) <= RELOCATION_MAX_DF
                }
                discr_size = sum(discr.values())
                if discr_size:
                    best_label, best_g = None, 0.0
                    for label, tokens in relocation_corpora:
                        g = sum(c for t, c in discr.items() if t in tokens) / discr_size
                        if g > best_g:
                            best_label, best_g = label, g
                    if best_g >= RELOCATION_THRESHOLD:
                        relocation_home = (best_label, round(best_g, 2))
            if relocation_home is not None:
                relocated_rows.append(
                    (f.raw_name, relocation_home[0], relocation_home[1], size))
                continue
            introduced_mass += size
            rows.append((f.raw_name, round(grounding, 2), size))
    precision = 1.0 - introduced_mass / total if total else 1.0
    return (precision, sorted(rows, key=lambda r: r[2], reverse=True),
            sorted(relocated_rows, key=lambda r: r[3], reverse=True))


def score_file(psalm: Doc, pz_methods: list[Method], pz_bag: Counter,
               idf: dict[str, float], intro_min_size: int = 8,
               intro_threshold: float = 0.2,
               relocation_corpora: list[tuple[str, set]] | None = None,
               relocation_df: dict[str, float] | None = None,
               ) -> tuple[float, float, dict]:
    """Return (file_recall, method_struct, detail).

    file_recall  -- of Psalm's whole-file weighted construct references, what
                    fraction is mirrored anywhere in the pzoom counterpart.
                    Robust to pzoom restructuring; this is the headline metric.
    method_struct -- weighted mean, over Psalm methods, of the reference recall
                    against the *same-named* pzoom method (0 when pzoom doesn't
                    keep a method of that name). Measures how far pzoom preserves
                    Psalm's method-level decomposition.

    detail also carries `precision` (1 − share of pzoom logic that is introduced
    behaviour absent from Psalm) and the `introduced` function list."""
    file_recall = weighted_recall(psalm.bag, pz_bag, idf)
    precision, introduced, relocated = find_introduced(
        pz_methods, psalm, set(idf), intro_min_size, intro_threshold,
        relocation_corpora, relocation_df)
    # Diagnostics only (never folded into the headline): file-level precision
    # flags relocated logic; novel share flags pzoom-only vocabulary/behaviour.
    file_precision = weighted_precision(psalm.bag, pz_bag, idf)
    novel_share = pz_novel_share(pz_bag, set(idf))

    num = den = 0.0
    matched = 0
    method_rows = []
    for pm in psalm.methods:
        wm = w_mass(pm.refs, idf)
        if wm == 0:
            continue
        cand = name_match(pm, pz_methods)
        if cand is not None:
            s = weighted_recall(pm.refs, cand.refs, idf)
            matched += 1
            kind = cand.raw_name
        else:
            s = 0.0
            kind = "—"
        num += wm * s
        den += wm
        method_rows.append((pm.raw_name, kind, s, wm))
    method_struct = num / den if den else 0.0
    # Construct references Psalm makes that pzoom does not mirror at all — the
    # actionable "implement these" hints, ranked by weighted frequency.
    missing = sorted(
        ((idf[t] * c, psalm.examples.get(t, t))
         for t, c in psalm.bag.items()
         if idf.get(t, 0.0) > 0 and pz_bag.get(t, 0) == 0),
        reverse=True,
    )
    detail = {
        "n_psalm_methods": len(method_rows),
        "n_name_matched": matched,
        "method_struct": method_struct,
        "methods": sorted(method_rows, key=lambda r: r[3], reverse=True),
        "missing_constructs": [name for _, name in missing[:14]],
        "precision": precision,
        "introduced": introduced,
        "relocated": relocated,
        "file_precision": file_precision,
        "novel_share": novel_share,
    }
    return file_recall, method_struct, detail


# --------------------------------------------------------------------------- #
# Main
# --------------------------------------------------------------------------- #


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pzoom-dir", default=None)
    ap.add_argument("--psalm-dir", default=None, help="Psalm repo root or its src/Psalm dir")
    ap.add_argument("--hakana-dir", default=None,
                    help="Hakana repo root (used to exonerate pzoom functions "
                         "ported from Hakana from the introduced penalty); "
                         "auto-located beside the Psalm clone when omitted")
    ap.add_argument("--mapping", default=None,
                    help="pzoom→Psalm file map JSON (default PSALM_FILE_MAP.json)")
    ap.add_argument("--report", default=None)
    ap.add_argument("--backlog", default=None,
                    help="path for the prioritized worklist (default docs/PSALM_PARITY_BACKLOG.md)")
    ap.add_argument("--json", default=None,
                    help="also write headline + per-file metrics as JSON to this path")
    ap.add_argument("--baseline", default=None,
                    help="JSON baseline to gate against; exit 1 on a per-file "
                         "recall regression (creates it if missing)")
    ap.add_argument("--update-baseline", action="store_true",
                    help="overwrite the --baseline file with current metrics")
    ap.add_argument("--tolerance", type=float, default=1.0,
                    help="per-file recall drop (points) tolerated before failing")
    ap.add_argument("--keep-interning", action="store_true",
                    help="do NOT drop interner/intern/StrId references")
    ap.add_argument("--dense-threshold", type=float, default=0.34,
                    help="min mapped-file fraction for a Psalm dir to count its "
                         "missing files as in-scope gaps (default 0.34)")
    ap.add_argument("--weight", choices=("binary", "idf"), default="binary",
                    help="construct-reference weighting (default binary)")
    ap.add_argument("--worst", type=int, default=25,
                    help="how many lowest-scoring matched files to detail")
    ap.add_argument("--intro-threshold", type=float, default=0.2,
                    help="a pzoom fn is 'introduced' if <this fraction of its refs "
                         "appear in the Psalm file (and it has no same-named method)")
    ap.add_argument("--intro-min-size", type=int, default=8,
                    help="ignore pzoom fns with fewer than this many refs when "
                         "looking for introduced methods")
    args = ap.parse_args()

    repo = Path(args.pzoom_dir).resolve() if args.pzoom_dir \
        else Path(__file__).resolve().parent.parent
    pz_root = repo / "crates"
    psalm_arg = args.psalm_dir
    if psalm_arg:
        p = Path(psalm_arg)
        psalm_root = p / "src" / "Psalm" if (p / "src" / "Psalm").exists() else p
    else:
        psalm_root = next((c for c in (Path("/home/user/psalm/src/Psalm"),
                                       Path.home() / "git/psalm/src/Psalm",
                                       repo.parent / "psalm/src/Psalm") if c.exists()), None)
    if not pz_root.exists() or psalm_root is None or not psalm_root.exists():
        print(f"error: pzoom={pz_root} psalm={psalm_root}", file=sys.stderr)
        return 2

    ki = args.keep_interning
    print("loading pzoom ...", file=sys.stderr)
    pz = load_docs(pz_root, ".rs", "rust", ki)
    pz = {rel: d for rel, d in pz.items()
          if rel.split("/", 1)[0] in SCOPE_CRATES}
    print("loading psalm ...", file=sys.stderr)
    ps = load_docs(psalm_root, ".php", "php", ki)

    hakana_root = None
    if args.hakana_dir:
        h = Path(args.hakana_dir)
        hakana_root = next((c for c in (h / "hakana-core" / "src", h / "src", h)
                            if c.exists()), None)
    else:
        hakana_root = next(
            (c for c in (Path.home() / "git/hakana/hakana-core/src",
                         repo.parent / "hakana/hakana-core/src") if c.exists()),
            None)
    hk: dict = {}
    if hakana_root is not None:
        print("loading hakana ...", file=sys.stderr)
        hk = load_docs(hakana_root, ".rs", "rust", ki)

    map_path = Path(args.mapping) if args.mapping else repo / "PSALM_FILE_MAP.json"
    inv = load_file_map(map_path)
    unknown = [p for p in inv if p not in ps]
    if unknown:
        print(f"warning: {len(unknown)} mapped Psalm files not found in the "
              f"Psalm checkout: {unknown[:5]}", file=sys.stderr)

    def pzoom_equiv(psalm_doc: Doc):
        """Return merged (methods, bag, label) for the pzoom counterpart, or None."""
        docs = []
        for r in inv.get(psalm_doc.rel, []):   # map paths are repo-relative
            key = r[len("crates/"):] if r.startswith("crates/") else r
            if key in pz:
                docs.append(pz[key])
        if not docs:
            return None
        methods, bag, labels = [], Counter(), []
        for d in docs:
            methods += d.methods
            bag.update(d.bag)
            labels.append(d.rel)
        return methods, bag, labels

    # A directory is "densely targeted" when pzoom mirrors a large fraction of
    # its Psalm files. Files that *have* a pzoom equivalent are always scored;
    # a *missing* Psalm file is only counted as an in-scope gap (score 0) when
    # it lives in a dense dir — otherwise pzoom is just cherry-picking from a
    # broad infra directory (Internal/, Internal/Codebase) and its unmapped
    # peers (PreloaderList, EventDispatcher, …) are out of scope, not gaps.
    dir_total: Counter = Counter()
    dir_mapped: Counter = Counter()
    for d in ps.values():
        parent = str(Path(d.rel).parent)
        dir_total[parent] += 1
        if d.rel in inv:
            dir_mapped[parent] += 1
    dense_dirs = {p for p in dir_total
                  if dir_mapped[p] >= 2
                  and dir_mapped[p] / dir_total[p] >= args.dense_threshold}
    in_scope_dirs = {p for p in dir_total if dir_mapped[p] >= 1}

    idf = build_weights(ps, args.weight)

    # Relocation corpora for the introduced check: every Psalm file's token
    # set (a flagged fn grounding strongly in a *different* Psalm file is
    # consolidated, not invented) plus Hakana's (sanctioned implementation
    # reference for e.g. dataflow machinery).
    # Data/generated files (e.g. PreloaderList.php: a flat list naming every
    # Psalm class) cannot be a behaviour "home" — they would blanket-exonerate
    # any function that references rare type names.
    def is_data_file(d) -> bool:
        return len(d.methods) <= 2 and len(set(d.bag)) > 400

    psalm_token_sets = [("psalm:" + d.rel, set(d.bag))
                        for d in ps.values() if not is_data_file(d)]
    hakana_token_sets = [("hakana:" + d.rel, set(d.bag))
                         for d in hk.values() if not is_data_file(d)]
    # Document frequency over the combined corpus, for the discriminative-token
    # filter in the relocation check.
    df_counts: Counter = Counter()
    for _, tokens in psalm_token_sets + hakana_token_sets:
        for t in tokens:
            df_counts[t] += 1
    n_corpus = max(1, len(psalm_token_sets) + len(hakana_token_sets))
    relocation_df = {t: c / n_corpus for t, c in df_counts.items()}

    rows = []
    for d in sorted(ps.values(), key=lambda x: x.rel):
        if d.rel in PORTED_OUTSIDE_SCOPE:
            continue
        mass = w_mass(d.bag, idf)
        if mass == 0:
            continue
        equiv = pzoom_equiv(d)
        if equiv is None:
            if str(Path(d.rel).parent) in dense_dirs:
                rows.append({"psalm": d.rel, "pz": None, "score": 0.0,
                             "struct": 0.0, "precision": 1.0, "penalized": 0.0,
                             "mass": mass, "detail": None})
            continue
        methods, bag, labels = equiv
        relocation_corpora = [
            (label, tokens) for label, tokens in psalm_token_sets
            if label != "psalm:" + d.rel
        ] + hakana_token_sets
        score, struct, detail = score_file(d, methods, bag, idf,
                                           args.intro_min_size, args.intro_threshold,
                                           relocation_corpora, relocation_df)
        precision = detail["precision"]
        rows.append({"psalm": d.rel, "pz": labels, "score": score,
                     "struct": struct, "precision": precision,
                     "penalized": score * precision, "mass": mass, "detail": detail})

    # Enrich relocations with actionable targets: the pzoom file mapped to the
    # behaviour's Psalm home (where the function should move), or a note that a
    # mapping is missing.
    for r in rows:
        d = r.get("detail")
        if not d or not d.get("relocated"):
            continue
        hints = []
        for name, home, g, size in d["relocated"]:
            if home.startswith("psalm:"):
                home_rel = home[len("psalm:"):]
                cells = inv.get(home_rel)
                hints.append((name, home_rel, g, size,
                              ", ".join(f"`{c}`" for c in cells) if cells
                              else "*(no mapping — out of the two-crate scope, "
                                   "or add a row to PSALM_FILE_MAP.json)*"))
            else:
                hints.append((name, home[len("hakana:"):], g, size, None))
        d["relocation_hints"] = hints

    total_mass = sum(r["mass"] for r in rows)
    matched_rows = [r for r in rows if r["pz"]]
    missing_rows = [r for r in rows if not r["pz"]]
    matched_mass = sum(r["mass"] for r in matched_rows)

    def wavg(key, subset, mass):
        return 100.0 * sum(r[key] * r["mass"] for r in subset) / mass if mass else 0.0

    # headline now folds in the introduced-method penalty (recall × precision)
    project = wavg("penalized", rows, total_mass)
    matched_only = wavg("score", matched_rows, matched_mass)        # recall
    matched_penalized = wavg("penalized", matched_rows, matched_mass)
    precision_only = wavg("precision", matched_rows, matched_mass)
    struct_only = wavg("struct", matched_rows, matched_mass)
    coverage = 100.0 * matched_mass / total_mass if total_mass else 0.0

    # Diagnostics only — NOT folded into the headline (see weighted_precision):
    def dwavg(detail_key, mass):
        return 100.0 * sum(r["detail"][detail_key] * r["mass"]
                           for r in matched_rows) / mass if mass else 0.0
    file_prec_diag = dwavg("file_precision", matched_mass)
    novel_diag = dwavg("novel_share", matched_mass)

    report = render(rows, project, matched_only, precision_only, struct_only,
                    coverage, in_scope_dirs, matched_rows, missing_rows, args,
                    file_prec_diag, novel_diag)
    out = Path(args.report) if args.report else repo / "docs" / "PSALM_PARITY_REPORT.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(report, encoding="utf-8")
    print(f"wrote {out}", file=sys.stderr)

    backlog = render_backlog(matched_rows, missing_rows, args)
    bpath = Path(args.backlog) if args.backlog else repo / "docs" / "PSALM_PARITY_BACKLOG.md"
    bpath.write_text(backlog, encoding="utf-8")
    print(f"wrote {bpath}", file=sys.stderr)

    n_introduced = sum(len(r["detail"]["introduced"]) for r in matched_rows)
    metrics = {
        "project": project, "matched_only": matched_only,
        "matched_penalized": matched_penalized, "precision": precision_only,
        "struct_only": struct_only, "coverage": coverage,
        "n_in_scope": len(rows), "n_matched": len(matched_rows),
        "n_missing": len(missing_rows), "n_introduced": n_introduced,
        # diagnostics (not part of the score): relocation + pzoom-only share.
        "file_precision_diag": file_prec_diag, "novel_share_diag": novel_diag,
        # per-file penalized score (recall × precision), so the regression gate
        # catches both lost Psalm coverage *and* newly-introduced divergence.
        "files": {r["psalm"]: round(100 * r["penalized"], 2) for r in matched_rows},
    }
    if args.json:
        Path(args.json).write_text(json.dumps(metrics, indent=2), encoding="utf-8")
        print(f"wrote {args.json}", file=sys.stderr)

    print(f"\nPsalm parity score:        {project:5.1f} / 100   "
          f"(in-scope Psalm files: {len(rows)})")
    print(f"  matched-files recall:    {matched_only:5.1f} / 100   "
          f"({len(matched_rows)} files)")
    print(f"  precision (1-introduced):{precision_only:5.1f} / 100   "
          f"({n_introduced} introduced fns flagged)")
    print(f"  matched penalized:       {matched_penalized:5.1f} / 100")
    print(f"  method-structure recall: {struct_only:5.1f} / 100")
    print(f"  file coverage (mass):    {coverage:5.1f}%   "
          f"({len(missing_rows)} in-scope files missing from pzoom)")
    print(f"  [diag] file-level precision: {file_prec_diag:5.1f} / 100   "
          f"(relocation signal — not scored)")
    print(f"  [diag] pzoom-only ref share: {novel_diag:5.1f}%   "
          f"(divergent vocab + introduced logic — not scored)")

    return regression_gate(metrics, args)


def regression_gate(metrics: dict, args) -> int:
    """Compare against a stored baseline; exit non-zero if parity regressed.
    Pinpoints any matched file whose recall dropped > --tolerance, so an
    accidental divergence from Psalm fails CI even if the aggregate holds."""
    if not args.baseline:
        return 0
    bp = Path(args.baseline)
    if args.update_baseline or not bp.exists():
        bp.parent.mkdir(parents=True, exist_ok=True)
        bp.write_text(json.dumps(metrics, indent=2), encoding="utf-8")
        print(f"\nwrote baseline {bp}", file=sys.stderr)
        return 0
    prev = json.loads(bp.read_text())
    tol = args.tolerance
    regressed = [
        (f, prev["files"][f], cur)
        for f, cur in metrics["files"].items()
        if f in prev.get("files", {}) and cur < prev["files"][f] - tol
    ]
    d = metrics["project"] - prev.get("project", metrics["project"])
    print(f"\nbaseline parity {prev.get('project', 0):.2f} → {metrics['project']:.2f} "
          f"({d:+.2f}); per-file tolerance {tol}")
    if regressed:
        print(f"REGRESSION in {len(regressed)} file(s):")
        for f, was, now in sorted(regressed, key=lambda x: x[2] - x[1])[:20]:
            print(f"  {was:5.1f} → {now:5.1f}  {f}")
        return 1
    print("OK — no per-file parity regression")
    return 0


def render_backlog(matched_rows, missing_rows, args) -> str:
    """A prioritized worklist that turns the score into development guidance.

    impact = reference-mass x (1 - recall): big files pzoom barely mirrors rank
    highest. Missing files (recall 0) are pure mass. For matched files we also
    list the specific Psalm constructs pzoom never references — concrete hints."""
    L = []
    a = L.append
    a("# Psalm ⇆ pzoom parity — development backlog\n")
    a("_Auto-generated by `scripts/psalm_parity.py`. Prioritised by "
      "**impact = reference-mass × (1 − recall)** — the most logic-heavy Psalm "
      "files pzoom mirrors least, first._\n")
    a("> ⚠️ This is a heuristic compass, not a target. A low score can mean "
      "\"faithfully ported but idiomatically different,\" and chasing the number "
      "by renaming Rust symbols to match Psalm tokens games it without improving "
      "behaviour (Goodhart). Use it to decide *where to look*, then judge by "
      "actual Psalm parity / tests.\n")

    for r in matched_rows:
        r["_impact"] = r["mass"] * (1 - r["score"])
    for r in missing_rows:
        r["_impact"] = r["mass"]

    a("## 1. Port next — in-scope Psalm files with no pzoom file\n")
    a("Whole files missing from a densely-mirrored directory, by reference mass.\n")
    a("| impact | Psalm file |")
    a("|---:|---|")
    for r in sorted(missing_rows, key=lambda r: r["_impact"], reverse=True)[:20]:
        a(f"| {r['_impact']:.0f} | `{r['psalm']}` |")
    a("")

    a("## 2. Deepen next — matched files that diverge most\n")
    a("pzoom has these, but mirrors little of Psalm's logic. Listed constructs "
      "are Psalm references pzoom never makes — the concrete things to implement "
      "(names are illustrative Psalm spellings; ignore any that are genuinely "
      "Rust-idiomatic differences).\n")
    a("| impact | recall | Psalm file → pzoom | missing constructs (top) |")
    a("|---:|---:|---|---|")
    deep = sorted(matched_rows, key=lambda r: r["_impact"], reverse=True)[:25]
    for r in deep:
        miss = ", ".join(f"`{m}`" for m in (r["detail"]["missing_constructs"] or [])[:8])
        pz = ", ".join(Path(p).name for p in r["pz"])
        a(f"| {r['_impact']:.0f} | {100*r['score']:.0f} | `{Path(r['psalm']).name}` → `{pz}` | {miss} |")
    a("")
    return "\n".join(L)


def render(rows, project, matched_only, precision_only, struct_only, coverage,
           in_scope_dirs, matched_rows, missing_rows, args,
           file_prec_diag=0.0, novel_diag=0.0) -> str:
    L = []
    a = L.append
    a("# Psalm ⇆ pzoom parity\n")
    a("_Generated by `scripts/psalm_parity.py`. The pzoom side is the "
      "`pzoom-analyzer` and `pzoom-code-info` crates, mapped file-by-file to "
      "Psalm in `PSALM_FILE_MAP.json`. For each in-scope Psalm file it "
      "finds the pzoom counterpart and scores **recall × precision**: recall = "
      "how many of Psalm's referenced members / methods / functions are mirrored "
      "on the pzoom side; precision = the share of pzoom's logic that corresponds "
      "to something in the Psalm file (penalising methods pzoom *introduces* that "
      "Psalm has under no naming). Identifiers are canonicalised "
      "(snake_case/PascalCase/camelCase, Storage→Info, getter→field, a synonym "
      "map, and PHP↔Rust idiom folds); interning and reporting plumbing are "
      "ignored. A Psalm file with no pzoom equivalent scores 0, weighted by its "
      "reference mass. **Scores are a relative signal, not an absolute %.**_\n")
    a(f"## Score (recall × precision): **{project:.1f} / 100**\n")
    a(f"- Matched-files-only recall: **{matched_only:.1f} / 100** "
      f"({len(matched_rows)} files)")
    a(f"- Precision (1 − introduced share): **{precision_only:.1f} / 100** "
      f"— penalises pzoom methods absent from the Psalm file under any naming")
    a(f"- Method-structure recall (same-named methods only): **{struct_only:.1f} / 100** "
      f"— how far pzoom keeps Psalm's per-method decomposition")
    a(f"- File coverage by reference mass: **{coverage:.1f}%** "
      f"({len(missing_rows)} in-scope Psalm files missing from pzoom)")
    a(f"- In-scope Psalm directories (≥1 mapped file): {len(in_scope_dirs)}")
    a(f"- interning: {'kept' if args.keep_interning else 'ignored'}\n")

    a("### Diagnostics (not folded into the score)\n")
    a("Two directional precision signals on matched files, deliberately **kept "
      "out of the headline** because each is confounded — useful for spotting "
      "*where* pzoom diverges, not for grading:\n")
    a(f"- **File-level precision: {file_prec_diag:.1f} / 100** — of the "
      "Psalm-vocabulary concepts a pzoom file references, the share its matched "
      "Psalm file *also* references. Tokens absent from Psalm carry zero weight, "
      "so this isolates **relocation**: a low value means pzoom pulls in concepts "
      "Psalm keeps in *other* files (or vice-versa). Not behaviour Psalm lacks.")
    a(f"- **pzoom-only reference share: {novel_diag:.1f}%** — fraction of a pzoom "
      "file's references absent from Psalm entirely. After plumbing is stripped "
      "this is mostly pzoom's divergent type-atom vocabulary plus genuinely "
      "introduced logic; high values flag files worth eyeballing.\n")

    a("## Biggest gaps — in-scope Psalm files with no pzoom equivalent\n")
    a("Sorted by IDF-weighted reference mass (≈ how much logic is unported).\n")
    a("| Psalm file | ref mass |")
    a("|---|---:|")
    for r in sorted(missing_rows, key=lambda r: r["mass"], reverse=True)[:30]:
        a(f"| `{r['psalm']}` | {r['mass']:.0f} |")
    a("")

    intro_rows = [r for r in matched_rows if r["detail"]["introduced"]]
    if intro_rows:
        a("## Introduced methods (pzoom-specific, absent from the Psalm file)\n")
        a("Functions pzoom defines that the corresponding Psalm file has under no "
          "naming — neither a same-named method nor one referencing the same "
          "constructs. These drive the precision penalty. Renames, decompositions "
          "of Psalm's inline logic, and Rust-idiom methods (`new`/`fmt`/…) are "
          "excluded; what remains is genuine divergence or relocated logic.\n")
        a("| Psalm file | precision | introduced functions (grounding) |")
        a("|---|---:|---|")
        for r in sorted(intro_rows, key=lambda r: r["precision"])[:30]:
            fns = ", ".join(f"`{n}` ({g:.2f})"
                            for n, g, _ in r["detail"]["introduced"][:6])
            a(f"| `{Path(r['psalm']).name}` | {100*r['precision']:.0f} | {fns} |")
        a("")

    # Render-time filter only (detail["relocation_hints"] keeps the full list):
    # tiny helpers ground "perfectly" in unrelated files by chance (twin
    # one-liners), so only hints of substantive size are worth acting on.
    MIN_RELOC_HINT_SIZE = 20
    renderable_hints = {
        id(r): [h for h in r["detail"].get("relocation_hints", [])
                if h[3] >= MIN_RELOC_HINT_SIZE]
        for r in matched_rows
    }
    reloc_rows = [r for r in matched_rows if renderable_hints[id(r)]]
    if reloc_rows:
        a("## Relocated logic (action: move it, or map it)\n")
        a("Functions exonerated from the introduced penalty because their "
          "behaviour grounds in a *different* Psalm file than the one their "
          "current Rust home is mapped to. For Psalm homes: move the function "
          "into the Rust counterpart listed under *move to* (or add a mapping "
          "row to PSALM_HAKANA_MAPPING.md when none exists) so file-level "
          "scores reflect where the logic actually lives. Hakana homes are "
          "sanctioned ports (AGENTS.md) and fine in place.\n")
        a("| current home (mapped to) | function | behaviour lives in | move to |")
        a("|---|---|---|---|")
        printed = 0
        for r in sorted(reloc_rows,
                        key=lambda r: -max(h[3] for h in renderable_hints[id(r)])):
            for name, home, g, size, target in renderable_hints[id(r)][:4]:
                if target is None:
                    target = f"*(Hakana port: `{home}` — keep in place)*"
                    home_label = f"hakana `{home}`"
                else:
                    home_label = f"`{home}`"
                home_cells = ", ".join(r["pz"]) if isinstance(r["pz"], list) else str(r["pz"])
                a(f"| `{home_cells}` (`{Path(r['psalm']).name}`) | `{name}` "
                  f"(g={g:.2f}, size={size}) | {home_label} | {target} |")
                printed += 1
                if printed >= 40:
                    break
            if printed >= 40:
                break
        a("")

    # `file-prec` and `novel` are diagnostics (relocation / pzoom-only share),
    # not part of recall × precision — see the Diagnostics section above.
    hdr = ("| Psalm file | pzoom | recall | prec | method-struct | "
           "methods matched | file-prec | novel |")
    sep = "|---|---|---:|---:|---:|---:|---:|---:|"

    def row(r):
        d = r["detail"]
        return (f"| `{r['psalm']}` | `{', '.join(r['pz'])}` | {100*r['score']:.0f} | "
                f"{100*r['precision']:.0f} | {100*r['struct']:.0f} | "
                f"{d['n_name_matched']}/{d['n_psalm_methods']} | "
                f"{100*d['file_precision']:.0f} | {100*d['novel_share']:.0f}% |")

    a("## Lowest-scoring matched files\n")
    a("Files pzoom has, but whose construct references diverge most from Psalm "
      "(candidates for closer alignment).\n")
    a(hdr)
    a(sep)
    for r in sorted(matched_rows, key=lambda r: r["penalized"])[:args.worst]:
        a(row(r))
    a("")

    a("## All matched files\n")
    a(hdr)
    a(sep)
    for r in sorted(matched_rows, key=lambda r: r["penalized"], reverse=True):
        a(row(r))
    a("")
    return "\n".join(L)


if __name__ == "__main__":
    sys.exit(main())
