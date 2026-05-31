# scripts/

## `similarity_heuristic.py`

Scores how closely each pzoom Rust source matches its counterpart in
[Psalm](https://github.com/vimeo/psalm) (PHP) and
[Hakana](https://github.com/slackhq/hakana) (Rust), the two codebases pzoom is
ported from.

### How it works

1. **Normalise identifiers.** Every identifier in every file is split into
   lowercase word tokens, folding `snake_case`, `PascalCase` and `camelCase` to
   the same form:

   ```
   ArrayFetchAnalyzer  ┐
   array_fetch_analyzer├─►  {array, fetch, analyzer}
   ```

   This makes the shared domain vocabulary (`TypeComparisonResult` ↔
   `type_comparison_result`, `ExpressionIdentifier` ↔ `expression_identifier`,
   `IssueKind`, …) directly comparable across PHP and Rust.

2. **TF-IDF cosine + filename Jaccard.** Each file becomes a TF-IDF vector over
   those tokens (IDF computed over the combined corpus so ubiquitous tokens like
   `analyzer` are down-weighted). The score blends the content cosine with a
   filename-token Jaccard (default 65% / 35%), scaled to 0–100.

3. **Auto-matching.** For every pzoom file the script ranks the entire Psalm and
   Hakana corpora and reports the best matches. It cross-checks these against the
   hard-coded `PSALM_HAKANA_MAPPING.md` (≈90% agreement), flags disagreements,
   and proposes counterparts for files the map leaves blank.

### Usage

```bash
# Auto-locates sibling psalm/ and hakana/ clones, or clones them:
python3 scripts/similarity_heuristic.py --clone

# Explicit reference paths:
python3 scripts/similarity_heuristic.py \
    --psalm-dir /path/to/psalm \
    --hakana-dir /path/to/hakana

# Machine-readable output + tuning:
python3 scripts/similarity_heuristic.py --json out.json --name-weight 0.4 --top 5
```

Writes a Markdown report to `docs/SIMILARITY_REPORT.md` by default (override with
`--report`). Pure standard library — no dependencies. Reference repos are located
via `--psalm-dir`/`--hakana-dir`, the `PSALM_DIR`/`HAKANA_DIR` env vars, common
sibling paths, or `--clone`.

## `psalm_parity.py`

A **Psalm-only**, **method-level** parity scorer — a sharper instrument than the
broad three-way `similarity_heuristic.py`. It answers: *how faithfully does each
pzoom file mirror the Psalm file it ports?*

For every in-scope Psalm file it finds the pzoom counterpart (from
`PSALM_HAKANA_MAPPING.md`, else an exact canonical-filename match) and scores
**recall × precision**:

- **recall** — what fraction of Psalm's referenced members / methods / functions
  are mirrored on the pzoom side (directional, so pzoom isn't punished for extra
  *detail* that elaborates Psalm's logic);
- **precision** — the share of pzoom's logic that corresponds to something in the
  Psalm file, penalising methods pzoom **introduces** that Psalm has under *no*
  naming.

Key mechanics:

- **Cross-language canonicalisation** of every identifier, so renames line up:
  - snake_case / PascalCase / camelCase → the same word tokens
  - `FunctionLikeStorage` (Psalm) == `FunctionLikeInfo` (pzoom)  *(Storage→Info rule)*
  - Psalm getter `getCodebase()` == pzoom field `codebase`        *(get- strip rule)*
  - a hand-curated **synonym map** for irregular pairs — `vars_in_scope`→`locals`,
    `$dim`→`offset`, `node_data`→`analysis_data`, … (extend `SYNONYMS` in the script).
- **PHP↔Rust idiom is folded out** so that renamings don't masquerade as logic
  gaps, letting *real* behavioural differences (e.g. Traversable handling) stand
  out. Two kinds:
  - *Folded* (same concept, different spelling): `TUnion`→`Union`, `TAtomic`→
    `Atomic`, pzoom `get_expr_type`/`set_expr_type` → Psalm `getType`/`setType`,
    Psalm `IssueBuffer::maybeAdd` → pzoom `add_issue`.
  - *Dropped* (reporting/location plumbing with no analog): Psalm builds a
    `CodeLocation` object and threads `getSuppressedIssues`/`getSource` where
    pzoom carries a byte span and emits an `IssueKind`. The *act* of emitting an
    issue is kept (via the `maybeAdd`→`add_issue` fold); only the bookkeeping is
    dropped.
- **Interning is ignored** (`interner`/`intern`/`StrId`) as a Rust-specific
  optimisation — and, symmetrically, so is Psalm-side framework plumbing with no
  analog (the immutable-`Union` `freeze`/`getBuilder` mechanism, the `PhpParser`
  AST library). `--keep-interning` disables all of the above plumbing drops.
- **Members and fields** are compared, not just method bodies, so data-holder
  structs (`*Info`/`*Storage`, scopes, result types) are scored on their fields.
- **Missing files are penalised hard**: a Psalm file with no pzoom equivalent
  scores 0, weighted by its reference mass. To avoid false gaps, a *missing*
  file only counts when it lives in a **densely-targeted** directory (one pzoom
  is mirroring file-for-file; tune with `--dense-threshold`); files that *exist*
  in pzoom are always scored regardless of directory.
- **Introduced-method penalty (precision)**: a pzoom function is flagged as
  *introduced* only when it is **both** differently-named from every Psalm method
  **and** poorly *grounded* — fewer than `--intro-threshold` (default 0.2) of its
  domain references appear in the Psalm file (a function referencing nothing
  Psalm knows is treated as fully novel). Grounding over *domain* tokens (those
  Psalm uses somewhere) plus exemptions for renames, decompositions, and
  Rust-idiom methods (`new`/`fmt`/`clone`/…) keep faithful helpers from being
  flagged; what's left is genuine divergence (e.g. pzoom's switch-exhaustiveness
  model, superglobal typing) or logic Psalm keeps in a different file. The
  penalty is **weighted by each flagged function's size** (its total reference
  count), so a large pzoom-specific method costs far more than a small one —
  and *removing* one visibly raises parity. The flagged functions are listed in
  the report's *Introduced methods* section.

Headline numbers: overall score (**recall × precision**, incl. the missing-file
penalty), matched-files recall, **precision** (1 − introduced share), and
**method-structure recall** (recall against the *same-named* pzoom method — how
far pzoom preserves Psalm's per-method decomposition).

Needs a local Psalm checkout: the script auto-locates a sibling `../psalm` (or
`$PSALM_DIR`); otherwise clone it once with
`git clone --depth 1 https://github.com/vimeo/psalm ../psalm`.

```bash
python3 scripts/psalm_parity.py                 # writes docs/PSALM_PARITY_REPORT.md
python3 scripts/psalm_parity.py --weight idf     # weight distinctive constructs higher
python3 scripts/psalm_parity.py --keep-interning --dense-threshold 0.5
```

### Distribution splits

Some Psalm "mega-files" are carved into several pzoom files. Where pzoom invents
an extra helper that has **no Psalm file of its own**, the parity scorer unions
that helper into the Psalm file's pzoom side — but **only if the same helper
also exists standalone in Hakana** (verified at runtime against the mapping's
Hakana column). That gates the union on a genuine cross-language structural
decision rather than a pzoom-only quirk. Currently:

- `StatementsAnalyzer.php` += `stmt_analyzer.rs`
- `FunctionLikeAnalyzer.php` += `function_analysis_data.rs`

(`function_analyzer.rs`/`class_analyzer.rs` are *not* unioned: Hakana keeps
`FunctionLikeAnalyzer` monolithic, so those splits are pzoom-only.) Edit
`DISTRIBUTION_SPLITS` in the script to add more; the Hakana check is automatic.

> **Reading the scores:** they are a *relative* signal (rank files, track over
> time, surface gaps), not an absolute percentage. Even faithful ports share
> only ~40–55% of per-construct vocabulary because Rust and PHP idioms differ,
> and pzoom intentionally distributes/thins some Psalm mega-files (see the notes
> in `PSALM_HAKANA_MAPPING.md`).

### Using the score to guide development

The score is most useful as a **compass**, in three ways:

1. **A prioritised backlog** (`docs/PSALM_PARITY_BACKLOG.md`, written on every
   run). Files are ranked by **impact = reference-mass × (1 − recall)** — the
   most logic-heavy Psalm files pzoom mirrors least, first. Two sections:
   *Port next* (whole Psalm files with no pzoom counterpart) and *Deepen next*
   (matched files that diverge most), the latter listing the specific Psalm
   constructs pzoom never references — concrete "implement these" hints, e.g.
   `AssertionFinder.php` → `getExtendedVarId`, `BinaryOp`, `ASSIGNMENT_TO_RIGHT`.

2. **A regression gate** for CI. Store a baseline and fail if any faithful-port
   file's recall drops, catching accidental divergence from Psalm:

   ```bash
   python3 scripts/psalm_parity.py --baseline .parity-baseline.json --update-baseline
   # later / in CI:
   python3 scripts/psalm_parity.py --baseline .parity-baseline.json --tolerance 1.0
   #   prints e.g. "baseline parity 16.09 -> 16.84 (+0.75)"
   #   exit 1 + the list of regressed files if any matched file slipped > 1.0 pt
   ```

   When you refresh the baseline after an intentional change, commit it
   alongside the reports it regenerates so they stay in lockstep:

   ```bash
   git add .parity-baseline.json docs/PSALM_PARITY_REPORT.md docs/PSALM_PARITY_BACKLOG.md
   ```

3. **The trend chart** (`parity_trend.py`, below) for direction over time.

> ⚠️ **Goodhart warning.** Don't optimise the number directly — renaming Rust
> symbols to match Psalm tokens raises recall without changing behaviour. Use it
> to decide *where to look*; judge the actual change by Psalm parity and tests.

## `parity_trend.py`

Replays `psalm_parity.py` (the HEAD version, against a fixed mapping for a
consistent frame) across every commit via a reused git worktree, and renders the
parity score, matched-files recall, and file coverage over history to
`docs/parity_trend.svg`.

```bash
python3 scripts/parity_trend.py                       # all commits
python3 scripts/parity_trend.py --max-commits 30      # last 30 only
```
