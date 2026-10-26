#!/usr/bin/env python3
"""
Structural test-coverage gate.

This is NOT line coverage. `cargo-llvm-cov` needs llvm-tools and an instrumented rebuild,
which is not wired up here, so instead of pretending, this checks the structure that line
coverage would reveal as missing: every module has a test module, every module has a
meaningful number of tests, and the suites that must exist for the product's claims exist.

It catches the real regression — someone adds `runtime.rs` or `security.rs` and forgets the
tests — which is the failure mode that actually happens.

Run: python3 scripts/check-test-coverage.py [--verbose]
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates" / "kavach-host" / "src"
INTEGRATION = ROOT / "crates" / "kavach-host" / "tests"

# Modules that are plumbing and carry their proof elsewhere.
NO_TESTS_REQUIRED = {"lib.rs"}
MIN_TESTS_PER_MODULE = 3
MIN_TOTAL_TESTS = 60

verbose = "--verbose" in sys.argv
problems: list[str] = []
rows: list[tuple[str, int, int]] = []

total = 0
for path in sorted(SRC.glob("*.rs")):
    text = path.read_text()
    tests = len(re.findall(r"#\[test\]", text))
    total += tests
    has_module = "#[cfg(test)]" in text
    rows.append((path.name, tests, 1 if has_module else 0))

    if path.name in NO_TESTS_REQUIRED:
        continue
    if not has_module:
        problems.append(f"{path.name} has no #[cfg(test)] module")
    elif tests < MIN_TESTS_PER_MODULE:
        problems.append(f"{path.name} has only {tests} test(s); at least {MIN_TESTS_PER_MODULE} expected")

integration = sorted(INTEGRATION.glob("*.rs")) if INTEGRATION.is_dir() else []
integration_tests = sum(len(re.findall(r"#\[test\]", p.read_text())) for p in integration)
total += integration_tests

# The claims that must have end-to-end proof, not just unit proof.
required_suites = {
    "local_runtime.rs": "that a local model is discovered and used",
    "crash_safety.rs": "that the store survives being killed mid-write",
}
for name, claim in required_suites.items():
    if not (INTEGRATION / name).exists():
        problems.append(f"missing integration suite {name}, which proves {claim}")

print("structural test coverage")
for name, tests, has_module in rows:
    flag = "" if has_module or name in NO_TESTS_REQUIRED else "   <-- no test module"
    if verbose:
        print(f"  {name:16s} {tests:3d} test(s){flag}")
print(f"  unit modules: {len(rows)}   integration suites: {len(integration)}")
print(f"  total tests: {total} (minimum {MIN_TOTAL_TESTS})")

if total < MIN_TOTAL_TESTS:
    problems.append(f"only {total} tests in total; at least {MIN_TOTAL_TESTS} expected")

if problems:
    print("\nPROBLEMS")
    for problem in problems:
        print(f"  - {problem}")
    print(f"\nRESULT: FAIL — {len(problems)} problem(s)")
    sys.exit(1)

print("\nRESULT: PASS — every module is tested and the required suites exist")
print("NOTE: structural only. Line coverage was not measured (cargo-llvm-cov needs crates.io).")
