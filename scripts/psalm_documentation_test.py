#!/usr/bin/env python3
"""Psalm DocumentationTest for pzoom.

Mirrors Psalm's ``tests/DocumentationTest.php``: crawl a Psalm checkout's
``docs/running_psalm/issues/*.md``, take the first ```php code block of each
issue's doc page, run pzoom over it, and assert pzoom reports that issue.

Per-issue knobs (PHP version, suppressed sibling issues, find-unused-code,
taint mode) replicate Psalm's ``providerInvalidCodeParse``. Issues Psalm
itself skips are skipped here too; issues that depend on config pzoom doesn't
expose are listed in UNSUPPORTED_CONFIG; genuine pzoom gaps live in
KNOWN_FAILURES so this script can gate CI — a known failure that starts
passing fails the run, keeping the list honest.

Usage:
    python3 scripts/psalm_documentation_test.py --psalm-dir /path/to/psalm \
        [--pzoom target/release/pzoom] [--filter IssueName] [--jobs 8]
"""

from __future__ import annotations

import argparse
import concurrent.futures
import re
import subprocess
import sys
import tempfile
from pathlib import Path

# Issues Psalm's own DocumentationTest skips in providerInvalidCodeParse.
PSALM_SKIPS = {
    "InvalidStringClass",
    "MissingThrowsDocblock",
    "PluginClass",
    "RedundantIdentityWithTrue",
    "TraitMethodSignatureMismatch",
    "UncaughtThrowInGlobalScope",
    "UnusedBaselineEntry",
    "UnusedIssueHandlerSuppression",
    "MethodSignatureMustProvideReturnType",  # @todo upstream: reinstate
}

# Issues whose docs deliberately carry no (testable) snippet upstream.
PSALM_NO_CODE = {
    "UnrecognizedExpression",
    "UnrecognizedStatement",
    "PluginIssue",
    "TaintedInput",
    "TaintedCustom",
    "ComplexFunction",
    "ComplexMethod",
    "ConfigIssue",
}

# Psalm enables these via Config flags pzoom's psalm.xml parser doesn't expose.
UNSUPPORTED_CONFIG = {
    "PossiblyUndefinedIntArrayOffset": "needs ensureArrayIntOffsetsExist",
    "PossiblyUndefinedStringArrayOffset": "needs ensureArrayStringOffsetsExist",
    "MissingOverrideAttribute": "needs ensureOverrideAttribute",
    "LiteralKeyUnshapedArray": "needs literal_array_key_check",
}

# Sibling issues Psalm suppresses so the headline issue is the one reported.
IGNORED_ISSUES = {
    "InvalidFalsableReturnType": ["FalsableReturnStatement"],
    "InvalidNullableReturnType": ["NullableReturnStatement"],
    "InvalidReturnType": ["InvalidReturnStatement"],
    "MixedStringOffsetAssignment": ["MixedAssignment"],
    "ParadoxicalCondition": ["MissingParamType"],
    "UnusedClass": ["UnusedVariable"],
    "UnusedMethod": ["UnusedVariable"],
    "ClassMustBeFinal": ["UnusedClass"],
}

PHP_81 = {
    "AmbiguousConstantInheritance",
    "DeprecatedConstant",
    "DuplicateEnumCase",
    "DuplicateEnumCaseValue",
    "InvalidEnumBackingType",
    "InvalidEnumCaseValue",
    "InvalidEnumMethod",
    "NoEnumProperties",
    "OverriddenFinalConstant",
    "InvalidInterfaceImplementation",
}
PHP_83 = {"InvalidOverride", "MissingOverrideAttribute", "MissingClassConstType"}

# pzoom reports a different (deliberately divergent) issue kind for these.
DIVERGENT_EXPECTATIONS = {
    # pzoom resolves names case-sensitively: a wrong-cased class is
    # UndefinedClass with the correct casing suggested, not InvalidClass.
    "InvalidClass": "UndefinedClass",
}

# Genuine pzoom gaps, discovered by this script. Each entry must keep
# failing: when one starts to pass, the run fails until it is removed.
KNOWN_FAILURES: dict[str, str] = {
    # Unused-declaration gaps the per-file pass (--find-unused-code) does not
    # yet cover:
    "UnusedConstructor": "constructor is skipped by the unused-method pass",
    "UnusedReturnValue": "no call-site return-value usage tracking",
    "UnusedPsalmSuppress": "a suppress over an issue-free statement is not tracked",
    # Scanner / declaration-level checks not yet implemented
    "ParseError": "parse errors are recovered, not surfaced as issues",
    # Include resolution is intentionally absent (no filesystem resolution)
    "MissingFile": "include path resolution unimplemented",
    "UnresolvableInclude": "include path resolution unimplemented",
}

ISSUE_LINE = re.compile(r"^(?:ERROR|INFO): ([A-Za-z]+) - ")


def extract_first_php_block(md_path: Path) -> tuple[str, str | None]:
    """Return (issue name from the # header, first ```php block or None)."""
    lines = md_path.read_text(encoding="utf-8").splitlines()
    issue = lines[0].replace("# ", "", 1).strip() if lines else md_path.stem
    block: list[str] | None = None
    for line in lines[1:]:
        if block is None:
            if line.startswith("```php"):
                block = []
        elif line.startswith("```"):
            return issue, "\n".join(block).strip()
        else:
            block.append(line)
    return issue, None


def needs_unused_code(issue: str) -> bool:
    """Mirror Psalm DocumentationTest's `check_references`: these issues are
    only emitted once the unused-declaration pass runs (Psalm's
    reportUnusedCode + consolidateAnalyzedData)."""
    return (
        issue == "ClassMustBeFinal"
        or "Unused" in issue
        or "Unevaluated" in issue
        or "Unnecessary" in issue
    )


def build_config(issue: str) -> str:
    attrs = ['errorLevel="1"']
    if issue in PHP_83:
        attrs.append('phpVersion="8.3"')
    elif issue in PHP_81:
        attrs.append('phpVersion="8.1"')
    else:
        attrs.append('phpVersion="8.0"')
    if needs_unused_code(issue):
        attrs.append('findUnusedCode="true"')
    if issue == "UnusedPsalmSuppress":
        attrs.append('findUnusedPsalmSuppress="true"')
    if "Tainted" in issue:
        attrs.append('runTaintAnalysis="true"')
    handlers = "".join(
        f'<{ignored} errorLevel="suppress" />'
        for ignored in IGNORED_ISSUES.get(issue, [])
    )
    return (
        '<?xml version="1.0"?>\n'
        f"<psalm {' '.join(attrs)}>\n"
        "  <projectFiles><directory name=\".\" /></projectFiles>\n"
        f"  <issueHandlers>{handlers}</issueHandlers>\n"
        "</psalm>\n"
    )


def run_one(pzoom: Path, issue: str, code: str) -> tuple[str, bool, str]:
    """Returns (issue, passed, detail)."""
    with tempfile.TemporaryDirectory(prefix=f"pzdoc-{issue}-") as tmp:
        tmp_path = Path(tmp)
        (tmp_path / "psalm.xml").write_text(build_config(issue), encoding="utf-8")
        (tmp_path / "input.php").write_text(code + "\n", encoding="utf-8")
        cmd = [str(pzoom), "--threads", "1"]
        # The doc snippet is a single self-contained file, so pzoom's per-file
        # reference tracking is whole-program here — enable the unused-declaration
        # pass that `findUnusedCode` leaves off for real multi-file projects.
        if needs_unused_code(issue):
            cmd.append("--find-unused-code")
        proc = subprocess.run(
            cmd,
            cwd=tmp_path,
            capture_output=True,
            text=True,
            timeout=120,
        )
        reported = []
        for line in proc.stdout.splitlines():
            m = ISSUE_LINE.match(line)
            if m:
                reported.append(m.group(1))
        expected = DIVERGENT_EXPECTATIONS.get(issue, issue)
        if expected in reported:
            return issue, True, ""
        detail = ", ".join(sorted(set(reported))) or "(no issues reported)"
        return issue, False, f"reported: {detail}"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--psalm-dir", required=True, help="Psalm checkout root")
    ap.add_argument("--pzoom", default=None,
                    help="pzoom binary (default <repo>/target/release/pzoom)")
    ap.add_argument("--filter", default=None, help="only run issues whose name contains this")
    ap.add_argument("--jobs", type=int, default=8)
    args = ap.parse_args()

    repo = Path(__file__).resolve().parent.parent
    pzoom = Path(args.pzoom) if args.pzoom else repo / "target/release/pzoom"
    if not pzoom.exists():
        print(f"error: pzoom binary not found at {pzoom}", file=sys.stderr)
        return 2
    issues_dir = Path(args.psalm_dir) / "docs" / "running_psalm" / "issues"
    if not issues_dir.is_dir():
        print(f"error: {issues_dir} not found", file=sys.stderr)
        return 2

    cases = []
    skipped = 0
    for md in sorted(issues_dir.glob("*.md")):
        issue, code = extract_first_php_block(md)
        if args.filter and args.filter not in issue:
            continue
        if issue in PSALM_SKIPS or issue in PSALM_NO_CODE or issue in UNSUPPORTED_CONFIG:
            skipped += 1
            continue
        if code is None:
            skipped += 1
            continue
        cases.append((issue, code))

    failures: list[tuple[str, str]] = []
    fixed_known: list[str] = []
    passed = 0
    xfail = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = [pool.submit(run_one, pzoom, issue, code) for issue, code in cases]
        for fut in concurrent.futures.as_completed(futures):
            issue, ok, detail = fut.result()
            if issue in KNOWN_FAILURES:
                if ok:
                    fixed_known.append(issue)
                else:
                    xfail += 1
            elif ok:
                passed += 1
            else:
                failures.append((issue, detail))

    for issue, detail in sorted(failures):
        print(f"FAIL {issue}: {detail}")
    for issue in sorted(fixed_known):
        print(f"FIXED {issue}: passes now — remove it from KNOWN_FAILURES")

    print(f"\n{passed} passed, {len(failures)} failed, {xfail} known failures, "
          f"{skipped} skipped, {len(fixed_known)} unexpectedly fixed "
          f"(of {len(cases)} testable issue docs)")
    return 1 if failures or fixed_known else 0


if __name__ == "__main__":
    sys.exit(main())
