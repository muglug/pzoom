#!/usr/bin/env python3
"""Maintain and enforce the pzoom-on-Psalm issue baseline.

The baseline lives in ``pzoom_psalm_audit.json`` (repo root): every issue pzoom
currently emits when run against the Psalm codebase, each tagged with a
classification (false_positive / false_negative / true_issue_psalm_misses /
real_but_psalm_suppresses_or_baselines).

Subcommands
-----------
check
    Run pzoom against a Psalm checkout (or read a captured pzoom report) and
    exit non-zero if pzoom emits any issue that is NOT already recorded in the
    baseline -- i.e. a new or regressed finding. This is what CI runs.

trim
    Remove baseline entries that pzoom no longer emits (stale entries), e.g.
    after a pzoom fix eliminates a false positive. Prints what would be removed;
    pass --write to rewrite the JSON (ids and classification_summary are
    recomputed).

Issue identity ignores line/column and normalises digit runs in the message, so
the baseline tolerates source-line shifts and metric-number drift (e.g.
ComplexMethod's graph-size numbers) while still catching genuinely new findings.
"""
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import Counter
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_BASELINE = REPO_ROOT / "pzoom_psalm_audit.json"

# ERROR: <Kind> - <file>:<line>:<col> - <message> (see https://psalm.dev/NNN)
ISSUE_RE = re.compile(
    r"^ERROR: (?P<kind>[A-Za-z]+) - (?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+) - "
    r"(?P<message>.*?)(?: \(see https://psalm\.dev/\d+\))?$"
)


def parse_pzoom_report(text: str) -> list[dict]:
    issues = []
    for raw in text.splitlines():
        m = ISSUE_RE.match(raw)
        if m:
            issues.append(
                {
                    "kind": m["kind"],
                    "file": m["file"],
                    "line": int(m["line"]),
                    "column": int(m["col"]),
                    "message": m["message"],
                }
            )
    return issues


def run_pzoom(pzoom_bin: str, project: str) -> str:
    proc = subprocess.run(
        [pzoom_bin, project], capture_output=True, text=True, check=False
    )
    if proc.returncode not in (0, 2):  # 2 == pzoom "errors found"
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"pzoom exited unexpectedly ({proc.returncode})")
    return proc.stdout


def identity(issue: dict, strip_prefix: str = "") -> tuple[str, str, str]:
    """Stable identity: (kind, file, digit-normalised message).

    Line/column are deliberately excluded so the baseline survives source moves;
    runs of digits in the message are normalised so metric numbers don't churn.
    """
    f = issue["file"]
    if strip_prefix and f.startswith(strip_prefix):
        f = f[len(strip_prefix):]
    norm_msg = re.sub(r"\d+", "#", issue["message"])
    return (issue["kind"], f, norm_msg)


def get_emitted(args) -> list[dict]:
    if args.pzoom_output:
        text = Path(args.pzoom_output).read_text()
    elif args.pzoom_bin and args.project:
        text = run_pzoom(args.pzoom_bin, args.project)
    else:
        raise SystemExit("provide --pzoom-output, or both --pzoom-bin and --project")
    return parse_pzoom_report(text)


def fmt(issue: dict) -> str:
    return f"  {issue['kind']} - {issue['file']}:{issue.get('line','?')} - {issue['message']}"


def cmd_check(args) -> int:
    data = json.loads(Path(args.baseline).read_text())
    baseline = data["issues"]
    emitted = get_emitted(args)

    remaining = Counter(identity(i, args.strip_prefix) for i in baseline)
    new_issues = []
    for issue in emitted:
        key = identity(issue, args.strip_prefix)
        if remaining[key] > 0:
            remaining[key] -= 1
        else:
            new_issues.append(issue)

    print(f"pzoom emitted {len(emitted)} issue(s); baseline records {len(baseline)}.")
    if new_issues:
        print(f"\n::error::{len(new_issues)} new pzoom issue(s) not in the baseline:")
        for issue in new_issues:
            print(fmt(issue))
        print(
            "\nIf these are expected, add them to pzoom_psalm_audit.json with a "
            "classification (or fix pzoom). If a pzoom fix removed old issues, run "
            "`scripts/pzoom_psalm_baseline.py trim --write`."
        )
        return 1
    print("OK: no new issues beyond the baseline.")
    return 0


def cmd_trim(args) -> int:
    path = Path(args.baseline)
    data = json.loads(path.read_text())
    baseline = data["issues"]
    emitted = get_emitted(args)

    available = Counter(identity(i, args.strip_prefix) for i in emitted)
    kept, stale = [], []
    for entry in baseline:
        key = identity(entry, args.strip_prefix)
        if available[key] > 0:
            available[key] -= 1
            kept.append(entry)
        else:
            stale.append(entry)

    if not stale:
        print("Baseline is already in sync: nothing to trim.")
        return 0

    print(f"{len(stale)} stale baseline entr(y/ies) no longer emitted by pzoom:")
    for entry in stale:
        print(fmt(entry))

    if not args.write:
        print("\n(dry run -- pass --write to update the baseline)")
        return 0

    for new_id, entry in enumerate(kept, 1):
        entry["id"] = new_id
    data["issues"] = kept
    data["observed_issue_count"] = len(kept)
    summary = Counter(e["classification"] for e in kept)
    data["classification_summary"] = {
        k: summary.get(k, 0) for k in data.get("classification_summary", summary)
    }
    for k in summary:
        data["classification_summary"].setdefault(k, summary[k])
    path.write_text(json.dumps(data, indent=2) + "\n")
    print(f"\nWrote {path} with {len(kept)} issue(s).")
    print("Note: review false_positive_root_causes / prose counts manually if present.")
    return 0


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for name in ("check", "trim"):
        sp = sub.add_parser(name)
        sp.add_argument("--baseline", default=str(DEFAULT_BASELINE))
        sp.add_argument("--pzoom-bin", help="path to the pzoom binary (run it against --project)")
        sp.add_argument("--project", help="path to the Psalm checkout to analyze")
        sp.add_argument("--pzoom-output", help="read a captured pzoom text report instead of running pzoom")
        sp.add_argument("--strip-prefix", default="", help="strip this leading path component from emitted file paths")
        if name == "trim":
            sp.add_argument("--write", action="store_true", help="rewrite the baseline JSON")
    args = p.parse_args()
    return cmd_check(args) if args.cmd == "check" else cmd_trim(args)


if __name__ == "__main__":
    raise SystemExit(main())
