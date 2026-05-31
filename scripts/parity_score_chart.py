#!/usr/bin/env python3
"""Render a focused single-line chart of just the Psalm⇆pzoom parity score.

Derives the series from the parity-score line of ``docs/parity_trend.svg`` (so
no re-scoring is needed — regenerate that first with ``parity_trend.py`` if you
want fresh data), then plots it on its own with an auto-zoomed y-axis so the
trajectory is readable instead of squashed against a 0–100 scale.

Usage:
    python3 scripts/parity_trend.py            # refresh the multi-line source
    python3 scripts/parity_score_chart.py      # then derive the parity-only view
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PARITY_COLOR = "#dc2626"  # the parity-score line in parity_trend.py


def extract_parity(svg: str) -> list[float]:
    """Recover the parity-score series from the trend SVG's red path, inverting
    the source chart's coordinate transform (geometry must match parity_trend)."""
    W, H, ML, MR, MT, MB = 1180, 600, 70, 240, 60, 110
    PH = H - MT - MB
    m = re.search(rf'<path d="([^"]+)" fill="none" stroke="{PARITY_COLOR}"', svg)
    if not m:
        sys.exit("no parity-score path found in source SVG")
    pts = re.findall(r"([\d.]+),([\d.]+)", m.group(1))
    return [round(100 * (1 - (float(y) - MT) / PH), 2) for _, y in pts]


def render(vals: list[float]) -> str:
    n = len(vals)
    W, H, ML, MR, MT, MB = 900, 420, 60, 30, 50, 60
    PW, PH = W - ML - MR, H - MT - MB
    lo = max(0.0, (min(vals) // 2) * 2 - 1)
    hi = (max(vals) // 2) * 2 + 2

    def X(i): return ML + PW * i / max(n - 1, 1)
    def Y(v): return MT + PH * (1 - (v - lo) / (hi - lo))

    P = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
         f'font-family="-apple-system,Segoe UI,Roboto,sans-serif">',
         f'<rect width="{W}" height="{H}" fill="#fff"/>',
         f'<text x="{ML}" y="28" font-size="18" font-weight="700" fill="#111">'
         f'Psalm ⇆ pzoom parity score</text>',
         f'<text x="{ML}" y="44" font-size="12" fill="#666">recall × precision, '
         f'mass-weighted over in-scope Psalm files — {n} commits (oldest → newest)</text>']
    g = lo
    while g <= hi + 1e-3:
        y = Y(g)
        P.append(f'<line x1="{ML}" y1="{y:.1f}" x2="{ML+PW}" y2="{y:.1f}" stroke="#eee"/>')
        P.append(f'<text x="{ML-8}" y="{y+4:.1f}" font-size="11" fill="#888" '
                 f'text-anchor="end">{g:.0f}</text>')
        g += 2
    P.append(f'<line x1="{ML}" y1="{MT}" x2="{ML}" y2="{MT+PH}" stroke="#bbb"/>')
    P.append(f'<line x1="{ML}" y1="{MT+PH}" x2="{ML+PW}" y2="{MT+PH}" stroke="#bbb"/>')
    d = "M " + " L ".join(f"{X(i):.1f},{Y(v):.1f}" for i, v in enumerate(vals))
    P.append(f'<path d="{d}" fill="none" stroke="{PARITY_COLOR}" stroke-width="2.2"/>')
    P.append("".join(f'<circle cx="{X(i):.1f}" cy="{Y(v):.1f}" r="2" '
                     f'fill="{PARITY_COLOR}"/>' for i, v in enumerate(vals)))
    lx, ly = X(n - 1), Y(vals[-1])
    P.append(f'<text x="{lx-6:.1f}" y="{ly-8:.1f}" font-size="12" font-weight="700" '
             f'fill="{PARITY_COLOR}" text-anchor="end">{vals[-1]:.1f}</text>')
    P.append(f'<text x="{ML+PW/2}" y="{H-15}" font-size="12" fill="#444" '
             f'text-anchor="middle">commit (oldest → newest)</text>')
    P.append("</svg>")
    return "\n".join(P)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--src", default=str(REPO / "docs" / "parity_trend.svg"))
    ap.add_argument("--out", default=str(REPO / "docs" / "parity_score.svg"))
    args = ap.parse_args()

    vals = extract_parity(Path(args.src).read_text(encoding="utf-8"))
    Path(args.out).write_text(render(vals), encoding="utf-8")
    print(f"wrote {args.out}  ({len(vals)} commits, "
          f"{min(vals):.1f}–{max(vals):.1f}, latest {vals[-1]:.1f})", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
