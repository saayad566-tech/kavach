#!/usr/bin/env bash
# Every verification that can run without crates.io, a compiler for the Tauri shell, or a
# Windows machine. If this passes, the remaining risk is concentrated in one place: the
# Tauri macros and the packaging step, which only a real build exercises.
#
#   ./scripts/verify-all.sh
#
# Exit code 0 means everything below passed.

set -uo pipefail

cd "$(dirname "$0")/.."
failures=0
run() {
  local name="$1"
  shift
  printf '\n=== %s ===\n' "$name"
  if "$@"; then
    printf 'PASS  %s\n' "$name"
  else
    printf 'FAIL  %s\n' "$name"
    failures=$((failures + 1))
  fi
}

# The container this was developed in caps virtual memory, which stops Node allocating its
# code range; raise it where the shell allows. Harmless on a normal machine.
ulimit -v unlimited 2>/dev/null || true

run "host tests (offline)" cargo test --offline --workspace
run "desktop configuration and packaging" python3 scripts/check-desktop-config.py --verbose
run "UI to command contract" python3 scripts/check-ui-contract.py
run "UI renders against a stubbed IPC" node scripts/verify-ui.mjs

printf '\n'
if [ "$failures" -eq 0 ]; then
  echo "RESULT: PASS — all offline checks passed"
  echo "NOT PROVEN: the Tauri shell compiles, a window renders, or an installer exists."
  exit 0
fi
echo "RESULT: FAIL — $failures check(s) failed"
exit 1
