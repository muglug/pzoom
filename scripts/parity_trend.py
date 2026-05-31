#!/usr/bin/env python3
"""Plot the Psalm⇆pzoom parity score across every commit as an SVG line chart.

Runs ``scripts/psalm_parity.py`` (the HEAD version, against a fixed mapping, so
the scoring frame is constant) at each commit via a reused git worktree, then
renders the project score, matched-files recall, and file coverage over history.

Usage:
    python3 scripts/parity_trend.py [--psalm-dir DIR] [--out docs/parity_trend.svg]
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent


def sh(args, **kw):
    return subprocess.run(args, capture_output=True, text=True, **kw)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--psalm-dir", default="/home/user/psalm")
    ap.add_argument("--out", default=str(REPO / "docs" / "parity_trend.svg"))
    ap.add_argument("--max-commits", type=int, default=0, help="0 = all")
    args = ap.parse_args()

    tmp = Path(tempfile.mkdtemp(prefix="parity_trend_"))
    scorer = tmp / "psalm_parity.py"
    mapping = tmp / "PSALM_HAKANA_MAPPING.md"
    shutil.copy(REPO / "scripts" / "psalm_parity.py", scorer)
    shutil.copy(REPO / "PSALM_HAKANA_MAPPING.md", mapping)
    wt = tmp / "wt"
    sh(["git", "-C", str(REPO), "worktree", "add", "--detach", "-f", str(wt), "HEAD"])

    commits = sh(["git", "-C", str(REPO), "rev-list", "--reverse", "HEAD"]).stdout.split()
    if args.max_commits:
        commits = commits[-args.max_commits:]

    series = []
    try:
        for i, sha in enumerate(commits):
            # Discard anything the scorer wrote into the worktree last iteration.
            # The scorer's backlog defaults to <pzoom-dir>/docs/, so without a
            # clean step (and the --backlog redirect below) that untracked file
            # blocks `git checkout` and silently freezes the replay on a stale
            # commit — making every later commit report identical scores.
            sh(["git", "-C", str(wt), "reset", "--hard", "-q"])
            sh(["git", "-C", str(wt), "clean", "-fdq"])
            sh(["git", "-C", str(wt), "checkout", "-q", "-f", "--detach", sha])
            landed = sh(["git", "-C", str(wt), "rev-parse", "HEAD"]).stdout.strip()
            subj = sh(["git", "-C", str(REPO), "log", "-1", "--format=%s", sha]).stdout.strip()
            rec = {"i": i, "sha": sha[:7], "subj": subj,
                   "project": None, "matched_only": None, "coverage": None,
                   "file_prec": None}
            if landed != sha:
                print(f"{i:2d} {sha[:7]} CHECKOUT FAILED (worktree on {landed[:7]})",
                      file=sys.stderr)
                series.append(rec)
                continue
            outj = tmp / "m.json"
            if outj.exists():
                outj.unlink()
            sh(["python3", str(scorer), "--pzoom-dir", str(wt),
                "--psalm-dir", args.psalm_dir, "--mapping", str(mapping),
                "--json", str(outj), "--report", str(tmp / "r.md"),
                "--backlog", str(tmp / "b.md")])
            if outj.exists():
                d = json.loads(outj.read_text())
                rec.update(project=d["project"], matched_only=d["matched_only"],
                           coverage=d["coverage"],
                           file_prec=d.get("file_precision_diag"))
            series.append(rec)
            p = rec["project"]
            print(f"{i:2d} {sha[:7]} parity={p if p is None else round(p,1)}  {subj[:48]}",
                  file=sys.stderr)
    finally:
        sh(["git", "-C", str(REPO), "worktree", "remove", "--force", str(wt)])

    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(render_svg(series), encoding="utf-8")
    print(f"\nwrote {args.out}", file=sys.stderr)
    return 0


def render_svg(series: list[dict]) -> str:
    W, H = 1180, 600
    ML, MR, MT, MB = 70, 240, 60, 110
    PW, PH = W - ML - MR, H - MT - MB
    n = len(series)
    xmax = max(n - 1, 1)

    def X(i):
        return ML + PW * i / xmax

    def Y(v):
        return MT + PH * (1 - v / 100.0)

    def line(key, color, dash=False):
        pts = [(X(s["i"]), Y(s[key])) for s in series if s[key] is not None]
        if not pts:
            return ""
        d = "M " + " L ".join(f"{x:.1f},{y:.1f}" for x, y in pts)
        dots = "".join(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="2.3" fill="{color}"/>'
                       for x, y in pts)
        da = ' stroke-dasharray="5,4"' if dash else ""
        return f'<path d="{d}" fill="none" stroke="{color}" stroke-width="2"{da}/>{dots}'

    P = []
    P.append(f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
             f'font-family="-apple-system,Segoe UI,Roboto,sans-serif">')
    P.append(f'<rect width="{W}" height="{H}" fill="#fff"/>')
    P.append(f'<text x="{ML}" y="28" font-size="18" font-weight="700" fill="#111">'
             f'Psalm ⇆ pzoom parity over {n} commits</text>')
    P.append(f'<text x="{ML}" y="46" font-size="12" fill="#666">Method-level construct-'
             f'reference recall (HEAD scorer + fixed mapping replayed over history)</text>')
    for g in range(0, 101, 20):
        y = Y(g)
        P.append(f'<line x1="{ML}" y1="{y:.1f}" x2="{ML+PW}" y2="{y:.1f}" stroke="#eee"/>')
        P.append(f'<text x="{ML-10}" y="{y+4:.1f}" font-size="11" fill="#888" '
                 f'text-anchor="end">{g}</text>')
    P.append(f'<line x1="{ML}" y1="{MT}" x2="{ML}" y2="{MT+PH}" stroke="#bbb"/>')
    P.append(f'<line x1="{ML}" y1="{MT+PH}" x2="{ML+PW}" y2="{MT+PH}" stroke="#bbb"/>')
    for s in series:
        if s["i"] % 5 == 0 or s["i"] == n - 1:
            x = X(s["i"])
            P.append(f'<line x1="{x:.1f}" y1="{MT+PH}" x2="{x:.1f}" y2="{MT+PH+5}" stroke="#bbb"/>')
            P.append(f'<text x="{x:.1f}" y="{MT+PH+20}" font-size="10" fill="#666" '
                     f'text-anchor="end" transform="rotate(-45 {x:.1f} {MT+PH+20})">'
                     f'{s["i"]}:{s["sha"]}</text>')

    cols = [("coverage", "#16a34a", "File coverage %", False),
            ("matched_only", "#2563eb", "Matched-files recall", False),
            ("project", "#dc2626", "Parity score (incl. penalty)", False),
            ("file_prec", "#9333ea", "File-level precision (diag)", True)]
    for key, color, _, dash in cols:
        P.append(line(key, color, dash))
    lx, ly = ML + PW + 20, MT
    for key, color, label, dash in cols:
        last = next((s[key] for s in reversed(series) if s[key] is not None), None)
        if dash:
            P.append(f'<line x1="{lx}" y1="{ly+7}" x2="{lx+14}" y2="{ly+7}" '
                     f'stroke="{color}" stroke-width="3" stroke-dasharray="5,4"/>')
        else:
            P.append(f'<rect x="{lx}" y="{ly}" width="14" height="14" fill="{color}"/>')
        P.append(f'<text x="{lx+20}" y="{ly+12}" font-size="12" fill="#111">{label}</text>')
        if last is not None:
            P.append(f'<text x="{lx+20}" y="{ly+27}" font-size="11" fill="#666">'
                     f'latest {last:.1f}</text>')
        ly += 46
    P.append(f'<text x="{ML+PW/2}" y="{H-15}" font-size="12" fill="#444" '
             f'text-anchor="middle">commit (oldest → newest)</text>')
    P.append('</svg>')
    return "\n".join(P)


if __name__ == "__main__":
    sys.exit(main())
