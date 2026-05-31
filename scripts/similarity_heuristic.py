#!/usr/bin/env python3
"""Score how closely each pzoom Rust source matches its Psalm / Hakana counterpart.

pzoom is a Rust port of two PHP-static-analyzer codebases:

  * Psalm  (PHP, PascalCase classes / camelCase methods) -- github.com/vimeo/psalm
  * Hakana (Rust, snake_case)                            -- github.com/slackhq/hakana

The three projects share a domain vocabulary (``ExpressionIdentifier`` /
``expression_identifier``, ``TypeComparisonResult`` / ``type_comparison_result``,
``IssueKind`` ...). After normalising snake_case, PascalCase and camelCase
identifiers down to a bag of lowercase word tokens, that vocabulary becomes
directly comparable across languages, so a TF-IDF cosine over those tokens is a
good cross-language similarity signal -- it does not care that one side is PHP
and the other Rust.

The script:

  1. Discovers every pzoom ``*.rs`` file plus the Psalm ``*.php`` and Hakana
     ``*.rs`` corpora (auto-locating sibling clones, or ``--clone``-ing them).
  2. Tokenises + normalises each file into a TF-IDF vector and a filename-token
     set.
  3. For each pzoom file:
       - scores the *known* counterpart from PSALM_HAKANA_MAPPING.md (if any);
       - independently *auto-matches* against the whole candidate corpus and
         reports the best matches, flagging where the auto-match disagrees with
         the hard-coded map and proposing matches for unmapped files.
  4. Emits a Markdown report (and optionally JSON).

Pure standard library -- no third-party dependencies.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path

# --------------------------------------------------------------------------- #
# Tokenisation / normalisation
# --------------------------------------------------------------------------- #

# Matches identifier-like runs. We deliberately ignore the rest of the source
# (operators, punctuation) -- the cross-language signal lives in the names.
_IDENT_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")

# Splits a single identifier into its sub-words, handling snake_case, kebab,
# camelCase, PascalCase, ACRONYMBoundaries and letter<->digit transitions.
#   ArrayFetchAnalyzer -> array fetch analyzer
#   array_fetch_analyzer -> array fetch analyzer
#   HTTPServer2 -> http server 2
_WORD_RE = re.compile(r"[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|\d+")

# Language syntax / primitive-type keywords carry no domain signal. Stripping
# them keeps the vectors focused on shared concepts; IDF then dampens whatever
# common noise remains. (Tokens shorter than --min-token-len are dropped too.)
STOPWORDS = frozenset(
    """
    fn let mut pub use crate self super impl struct enum trait mod const static
    ref dyn move async await match if else for while loop break continue return
    where box dyn unsafe extern type as into from clone copy default debug eq ord
    hash send sync sized drop iter map filter collect push pop insert remove get
    set new with len str string bool char vec option some none ok err result
    usize isize true false todo unimplemented unreachable println format write
    php function public private protected abstract final namespace class interface
    trait extends implements echo array int float void null this parent foreach
    endif elseif while do switch case break continue throw try catch finally
    instanceof global isset unset list and or xor declare strict types var print
    """.split()
)


def split_identifier(ident: str) -> list[str]:
    """Break a single identifier into normalised lowercase sub-word tokens."""
    return [w.lower() for w in _WORD_RE.findall(ident)]


def tokenize(text: str, min_len: int) -> Counter:
    """Return a term-frequency multiset of normalised identifier sub-words."""
    counts: Counter = Counter()
    for ident in _IDENT_RE.findall(text):
        for word in split_identifier(ident):
            if len(word) >= min_len and word not in STOPWORDS:
                counts[word] += 1
    return counts


def name_tokens(path: Path, min_len: int) -> set[str]:
    """Normalised token set for a file's basename (sans extension)."""
    return {
        w
        for w in split_identifier(path.stem)
        if len(w) >= min_len and w not in STOPWORDS
    }


# --------------------------------------------------------------------------- #
# Vector model
# --------------------------------------------------------------------------- #


@dataclass
class Doc:
    path: Path
    rel: str            # display path
    tf: Counter         # raw term frequencies
    names: set[str]     # basename tokens
    vec: dict = field(default_factory=dict)   # tf-idf vector (filled later)
    norm: float = 0.0   # L2 norm of vec


def build_idf(docs: list[Doc]) -> dict[str, float]:
    """Inverse document frequency across the given corpus."""
    df: Counter = Counter()
    for d in docs:
        df.update(d.tf.keys())
    n = max(len(docs), 1)
    return {term: math.log((n + 1) / (freq + 1)) + 1.0 for term, freq in df.items()}


def apply_tfidf(doc: Doc, idf: dict[str, float]) -> None:
    """Fill in a log-scaled TF-IDF vector and its norm for *doc*."""
    vec = {}
    for term, freq in doc.tf.items():
        weight = idf.get(term)
        if weight:
            vec[term] = (1.0 + math.log(freq)) * weight
    doc.vec = vec
    doc.norm = math.sqrt(sum(v * v for v in vec.values()))


def cosine(a: Doc, b: Doc) -> float:
    """Cosine similarity between two TF-IDF vectors (0..1)."""
    if a.norm == 0.0 or b.norm == 0.0:
        return 0.0
    # iterate the smaller vector
    small, large = (a.vec, b.vec) if len(a.vec) <= len(b.vec) else (b.vec, a.vec)
    dot = sum(w * large.get(t, 0.0) for t, w in small.items())
    return dot / (a.norm * b.norm)


def name_jaccard(a: set[str], b: set[str]) -> float:
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def combined_score(pz: Doc, cand: Doc, name_weight: float) -> tuple[float, float, float]:
    """Return (score, content_cosine, name_jaccard), each scaled 0..100 for score."""
    content = cosine(pz, cand)
    name = name_jaccard(pz.names, cand.names)
    score = 100.0 * ((1.0 - name_weight) * content + name_weight * name)
    return score, content * 100.0, name * 100.0


# --------------------------------------------------------------------------- #
# Corpus discovery
# --------------------------------------------------------------------------- #


def load_docs(root: Path, ext: str, min_len: int) -> list[Doc]:
    docs = []
    for path in sorted(root.rglob(f"*{ext}")):
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        docs.append(
            Doc(
                path=path,
                rel=str(path),
                tf=tokenize(text, min_len),
                names=name_tokens(path, min_len),
            )
        )
    return docs


def first_existing(*candidates: Path) -> Path | None:
    for c in candidates:
        if c and c.exists():
            return c
    return None


def locate_psalm(arg: str | None) -> Path | None:
    if arg:
        p = Path(arg)
        return p / "src" / "Psalm" if (p / "src" / "Psalm").exists() else p
    env = os.environ.get("PSALM_DIR")
    return first_existing(
        Path(env) / "src" / "Psalm" if env else None,
        Path("/home/user/psalm/src/Psalm"),
        Path.home() / "git" / "psalm" / "src" / "Psalm",
        Path(__file__).resolve().parent.parent.parent / "psalm" / "src" / "Psalm",
    )


def locate_hakana(arg: str | None) -> Path | None:
    """Hakana's analyzer sources -- handle both the modern ``src/`` layout and
    the older ``hakana-core/src/`` layout referenced in the mapping doc."""
    roots: list[Path] = []
    if arg:
        roots.append(Path(arg))
    if os.environ.get("HAKANA_DIR"):
        roots.append(Path(os.environ["HAKANA_DIR"]))
    roots += [
        Path("/home/user/hakana"),
        Path.home() / "git" / "hakana",
        Path(__file__).resolve().parent.parent.parent / "hakana",
    ]
    for r in roots:
        for sub in ("hakana-core/src", "src"):
            if (r / sub).exists():
                return r / sub
        if r.exists() and r.name == "src":
            return r
    return None


def clone_repo(url: str, dest: Path) -> Path | None:
    if dest.exists():
        return dest
    print(f"  cloning {url} -> {dest} ...", file=sys.stderr)
    try:
        subprocess.run(
            ["git", "clone", "--depth", "1", url, str(dest)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return dest
    except (subprocess.CalledProcessError, OSError) as exc:
        print(f"  clone failed: {exc}", file=sys.stderr)
        return None


# --------------------------------------------------------------------------- #
# Hard-coded mapping (PSALM_HAKANA_MAPPING.md)
# --------------------------------------------------------------------------- #

# Names the mapping uses for "no real file here".
_PLACEHOLDER = {"", "(module root)", "[stub]"}


def _clean_cell(cell: str) -> str:
    """Normalise a mapping-table cell to a bare ``Foo.php`` / ``foo.rs`` name."""
    cell = cell.strip().strip("`").strip()
    if cell.lower() in _PLACEHOLDER:
        return ""
    # "ParseTree.php (+ ParseTree/* subclasses)" -> "ParseTree.php"
    m = re.match(r"([A-Za-z0-9_]+\.(?:php|rs))", cell)
    return m.group(1) if m else ""


def parse_mapping(path: Path) -> dict[str, dict[str, str]]:
    """pzoom-rel-path -> {'hakana': name|'', 'psalm': name|''}."""
    mapping: dict[str, dict[str, str]] = {}
    if not path.exists():
        return mapping
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("| `crates/"):
            continue
        cols = [c for c in line.split("|")]
        # | pzoom | hakana | psalm |  -> indices 1,2,3
        if len(cols) < 4:
            continue
        pz = cols[1].strip().strip("`").strip()
        mapping[pz] = {
            "hakana": _clean_cell(cols[2]),
            "psalm": _clean_cell(cols[3]),
        }
    return mapping


def resolve_named(name: str, by_basename: dict[str, list[Doc]], pz: Doc,
                  name_weight: float) -> Doc | None:
    """Resolve a mapping basename to a concrete Doc, disambiguating duplicate
    basenames by picking the highest-scoring candidate against *pz*."""
    if not name:
        return None
    cands = by_basename.get(name.lower())
    if not cands:
        return None
    if len(cands) == 1:
        return cands[0]
    return max(cands, key=lambda c: combined_score(pz, c, name_weight)[0])


# --------------------------------------------------------------------------- #
# Matching
# --------------------------------------------------------------------------- #


def best_matches(pz: Doc, candidates: list[Doc], name_weight: float, top: int):
    scored = [
        (combined_score(pz, c, name_weight), c) for c in candidates
    ]
    scored.sort(key=lambda x: x[0][0], reverse=True)
    return scored[:top]


# --------------------------------------------------------------------------- #
# Reporting
# --------------------------------------------------------------------------- #


def fmt_score(s: float) -> str:
    return f"{s:5.1f}"


def build_report(results: list[dict], cfg: dict) -> str:
    lines: list[str] = []
    w = lines.append

    w("# pzoom ↔ Psalm / Hakana similarity heuristic\n")
    w("_Generated by `scripts/similarity_heuristic.py`. Scores are 0–100, "
      "combining a TF-IDF cosine over normalised identifier tokens with a "
      "filename-token Jaccard. snake_case / PascalCase / camelCase are folded "
      "to the same lowercase word tokens, so the score is language-agnostic._\n")

    w("## Configuration\n")
    w(f"- pzoom files: **{cfg['n_pzoom']}**")
    w(f"- Psalm corpus: **{cfg['n_psalm']}** files (`{cfg['psalm_dir']}`)")
    w(f"- Hakana corpus: **{cfg['n_hakana']}** files (`{cfg['hakana_dir']}`)")
    w(f"- name weight: {cfg['name_weight']} · content weight: "
      f"{1 - cfg['name_weight']:.2f} · min token length: {cfg['min_len']}\n")

    # ---- aggregate stats --------------------------------------------------- #
    def avg(key):
        vals = [r[key] for r in results if r[key] is not None]
        return sum(vals) / len(vals) if vals else 0.0

    mapped_hk = [r for r in results if r["map_hakana_score"] is not None]
    mapped_ps = [r for r in results if r["map_psalm_score"] is not None]
    hk_agree = sum(1 for r in mapped_hk if r["hakana_agree"])
    ps_agree = sum(1 for r in mapped_ps if r["psalm_agree"])

    w("## Summary\n")
    w(f"- Mean score vs mapped Hakana file: **{fmt_score(avg('map_hakana_score'))}** "
      f"(over {len(mapped_hk)} mapped files)")
    w(f"- Mean score vs mapped Psalm file: **{fmt_score(avg('map_psalm_score'))}** "
      f"(over {len(mapped_ps)} mapped files)")
    if mapped_hk:
        w(f"- Auto-match top-1 agrees with the hard-coded Hakana map: "
          f"**{hk_agree}/{len(mapped_hk)}** ({100*hk_agree/len(mapped_hk):.0f}%)")
    if mapped_ps:
        w(f"- Auto-match top-1 agrees with the hard-coded Psalm map: "
          f"**{ps_agree}/{len(mapped_ps)}** ({100*ps_agree/len(mapped_ps):.0f}%)")
    w("")

    # ---- disagreements (auto-match != hard-coded map) ---------------------- #
    disagree = [r for r in results
                if (r["map_hakana_score"] is not None and not r["hakana_agree"])
                or (r["map_psalm_score"] is not None and not r["psalm_agree"])]
    if disagree:
        w("## ⚠️ Auto-match disagrees with the hard-coded map\n")
        w("Worth a look: either the map is stale, or the heuristic is being "
          "fooled by shared vocabulary. Sorted by mapped-score gap.\n")
        w("| pzoom file | side | mapped (score) | auto top-1 (score) |")
        w("|---|---|---|---|")
        for r in sorted(disagree, key=lambda r: r["max_gap"], reverse=True):
            for side in ("hakana", "psalm"):
                if r[f"map_{side}_score"] is None or r[f"{side}_agree"]:
                    continue
                top = r[f"{side}_top"][0] if r[f"{side}_top"] else None
                w(f"| `{r['pzoom']}` | {side} | `{r[f'map_{side}_name']}` "
                  f"({fmt_score(r[f'map_{side}_score'])}) | "
                  f"`{top['name']}` ({fmt_score(top['score'])}) |"
                  if top else
                  f"| `{r['pzoom']}` | {side} | `{r[f'map_{side}_name']}` "
                  f"({fmt_score(r[f'map_{side}_score'])}) | — |")
        w("")

    # ---- proposed mappings for unmapped pzoom files ------------------------ #
    proposals = [r for r in results
                 if (r["map_hakana_score"] is None and r["hakana_top"])
                 or (r["map_psalm_score"] is None and r["psalm_top"])]
    if proposals:
        w("## Proposed mappings (pzoom files blank in the map)\n")
        w("Top auto-match for files with no hard-coded counterpart. Treat low "
          "scores as 'probably pzoom-specific'.\n")
        w("| pzoom file | Hakana candidate (score) | Psalm candidate (score) |")
        w("|---|---|---|")
        for r in sorted(proposals, key=lambda r: r["pzoom"]):
            hk = (r["hakana_top"][0] if (r["map_hakana_score"] is None and r["hakana_top"])
                  else None)
            ps = (r["psalm_top"][0] if (r["map_psalm_score"] is None and r["psalm_top"])
                  else None)
            hk_s = f"`{hk['name']}` ({fmt_score(hk['score'])})" if hk else "—"
            ps_s = f"`{ps['name']}` ({fmt_score(ps['score'])})" if ps else "—"
            w(f"| `{r['pzoom']}` | {hk_s} | {ps_s} |")
        w("")

    # ---- full per-file table ---------------------------------------------- #
    w("## All files\n")
    w("`map` = score vs the hard-coded counterpart; `auto` = best auto-matched "
      "candidate. ✓ = auto-match agrees with the map.\n")
    w("| pzoom file | Hakana map | Hakana auto | Psalm map | Psalm auto |")
    w("|---|---|---|---|---|")
    for r in sorted(results, key=lambda r: r["pzoom"]):
        def cell(side):
            ms = r[f"map_{side}_score"]
            map_part = (f"`{r[f'map_{side}_name']}` {fmt_score(ms)}"
                        if ms is not None else "—")
            top = r[f"{side}_top"][0] if r[f"{side}_top"] else None
            auto_part = (f"`{top['name']}` {fmt_score(top['score'])}"
                         f"{' ✓' if r[f'{side}_agree'] else ''}"
                         if top else "—")
            return map_part, auto_part
        hk_map, hk_auto = cell("hakana")
        ps_map, ps_auto = cell("psalm")
        w(f"| `{r['pzoom']}` | {hk_map} | {hk_auto} | {ps_map} | {ps_auto} |")
    w("")
    return "\n".join(lines)


# --------------------------------------------------------------------------- #
# Main
# --------------------------------------------------------------------------- #


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pzoom-dir", default=None,
                    help="pzoom repo root (default: repo containing this script)")
    ap.add_argument("--psalm-dir", default=None,
                    help="Psalm repo root or its src/Psalm dir")
    ap.add_argument("--hakana-dir", default=None, help="Hakana repo root")
    ap.add_argument("--mapping", default=None,
                    help="path to PSALM_HAKANA_MAPPING.md")
    ap.add_argument("--report", default=None,
                    help="output Markdown report path (default: docs/SIMILARITY_REPORT.md)")
    ap.add_argument("--json", default=None, help="also write machine-readable JSON here")
    ap.add_argument("--top", type=int, default=3, help="auto-match candidates to keep")
    ap.add_argument("--name-weight", type=float, default=0.35,
                    help="0..1 weight of filename-token similarity (rest is content)")
    ap.add_argument("--min-token-len", type=int, default=3,
                    help="drop normalised tokens shorter than this")
    ap.add_argument("--clone", action="store_true",
                    help="git clone Psalm/Hakana into sibling dirs if not found")
    args = ap.parse_args()

    repo_root = Path(args.pzoom_dir).resolve() if args.pzoom_dir \
        else Path(__file__).resolve().parent.parent
    pzoom_src = repo_root / "crates"
    if not pzoom_src.exists():
        print(f"error: no crates/ dir under {repo_root}", file=sys.stderr)
        return 2

    psalm_dir = locate_psalm(args.psalm_dir)
    hakana_dir = locate_hakana(args.hakana_dir)

    if args.clone:
        if psalm_dir is None:
            d = clone_repo("https://github.com/vimeo/psalm.git",
                           repo_root.parent / "psalm")
            psalm_dir = (d / "src" / "Psalm") if d else None
        if hakana_dir is None:
            d = clone_repo("https://github.com/slackhq/hakana.git",
                           repo_root.parent / "hakana")
            hakana_dir = (d / "src") if d else None

    if psalm_dir is None or hakana_dir is None:
        print("error: could not locate reference repos.\n"
              f"  psalm:  {psalm_dir}\n  hakana: {hakana_dir}\n"
              "Pass --psalm-dir/--hakana-dir or use --clone.", file=sys.stderr)
        return 2

    min_len = args.min_token_len
    print(f"loading pzoom  ({pzoom_src}) ...", file=sys.stderr)
    pzoom_docs = load_docs(pzoom_src, ".rs", min_len)
    print(f"loading psalm  ({psalm_dir}) ...", file=sys.stderr)
    psalm_docs = load_docs(psalm_dir, ".php", min_len)
    print(f"loading hakana ({hakana_dir}) ...", file=sys.stderr)
    hakana_docs = load_docs(hakana_dir, ".rs", min_len)

    # make pzoom rel-paths repo-relative to match the mapping doc
    for d in pzoom_docs:
        try:
            d.rel = str(d.path.relative_to(repo_root))
        except ValueError:
            d.rel = str(d.path)

    # Build a shared IDF so a token's weight is consistent across corpora; this
    # keeps cross-language cosine meaningful (a term ubiquitous in one corpus is
    # still down-weighted).
    idf = build_idf(pzoom_docs + psalm_docs + hakana_docs)
    for d in pzoom_docs + psalm_docs + hakana_docs:
        apply_tfidf(d, idf)

    psalm_by_name: dict[str, list[Doc]] = defaultdict(list)
    for d in psalm_docs:
        psalm_by_name[d.path.name.lower()].append(d)
    hakana_by_name: dict[str, list[Doc]] = defaultdict(list)
    for d in hakana_docs:
        hakana_by_name[d.path.name.lower()].append(d)

    mapping = parse_mapping(Path(args.mapping) if args.mapping
                            else repo_root / "PSALM_HAKANA_MAPPING.md")
    nw = args.name_weight

    results = []
    for pz in pzoom_docs:
        m = mapping.get(pz.rel, {})

        def side(name_key: str, by_name, cands) -> dict:
            mapped_name = m.get(name_key, "")
            mapped_doc = resolve_named(mapped_name, by_name, pz, nw)
            mapped_score = (combined_score(pz, mapped_doc, nw)[0]
                            if mapped_doc else None)
            tops = [
                {"name": c.path.name, "rel": c.rel, "score": sc[0],
                 "content": sc[1], "name_sim": sc[2]}
                for sc, c in best_matches(pz, cands, nw, args.top)
            ]
            agree = bool(mapped_doc and tops
                         and tops[0]["rel"] == mapped_doc.rel)
            return {
                "name": mapped_name,
                "score": mapped_score,
                "top": tops,
                "agree": agree,
            }

        hk = side("hakana", hakana_by_name, hakana_docs)
        ps = side("psalm", psalm_by_name, psalm_docs)

        gap = 0.0
        for s in (hk, ps):
            if s["score"] is not None and s["top"]:
                gap = max(gap, s["top"][0]["score"] - s["score"])

        results.append({
            "pzoom": pz.rel,
            "map_hakana_name": hk["name"], "map_hakana_score": hk["score"],
            "hakana_top": hk["top"], "hakana_agree": hk["agree"],
            "map_psalm_name": ps["name"], "map_psalm_score": ps["score"],
            "psalm_top": ps["top"], "psalm_agree": ps["agree"],
            "max_gap": gap,
        })

    cfg = {
        "n_pzoom": len(pzoom_docs), "n_psalm": len(psalm_docs),
        "n_hakana": len(hakana_docs), "psalm_dir": str(psalm_dir),
        "hakana_dir": str(hakana_dir), "name_weight": nw, "min_len": min_len,
    }

    report = build_report(results, cfg)
    report_path = Path(args.report) if args.report else repo_root / "docs" / "SIMILARITY_REPORT.md"
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(report, encoding="utf-8")
    print(f"wrote {report_path}", file=sys.stderr)

    if args.json:
        Path(args.json).write_text(
            json.dumps({"config": cfg, "results": results}, indent=2),
            encoding="utf-8")
        print(f"wrote {args.json}", file=sys.stderr)

    # console summary
    mh = [r["map_hakana_score"] for r in results if r["map_hakana_score"] is not None]
    mp = [r["map_psalm_score"] for r in results if r["map_psalm_score"] is not None]
    print(f"\npzoom={len(pzoom_docs)} psalm={len(psalm_docs)} hakana={len(hakana_docs)}")
    if mh:
        print(f"mean score vs mapped Hakana: {sum(mh)/len(mh):.1f} ({len(mh)} files)")
    if mp:
        print(f"mean score vs mapped Psalm:  {sum(mp)/len(mp):.1f} ({len(mp)} files)")
    hk_ag = sum(1 for r in results if r["map_hakana_score"] is not None and r["hakana_agree"])
    ps_ag = sum(1 for r in results if r["map_psalm_score"] is not None and r["psalm_agree"])
    if mh:
        print(f"auto-match agrees with Hakana map: {hk_ag}/{len(mh)} ({100*hk_ag/len(mh):.0f}%)")
    if mp:
        print(f"auto-match agrees with Psalm map:  {ps_ag}/{len(mp)} ({100*ps_ag/len(mp):.0f}%)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
