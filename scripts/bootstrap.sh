#!/usr/bin/env bash
# Set this machine up to build and verify Kavach — once.
#
# Everything here is free and needs no account. Run it after cloning:
#
#   ./scripts/bootstrap.sh
#
# It is safe to run again: each step checks before it acts.

set -uo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
ok()  { printf '  ok    %s\n' "$*"; }
note(){ printf '  note  %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*"; }

failures=0

# ---------------------------------------------------------------- 1. tools
say "1/4  Required tools"

need() { # name, minimum-major, install hint
  local name="$1" major="$2" hint="$3"
  if ! command -v "$name" >/dev/null 2>&1; then
    bad "$name is missing — $hint"
    failures=$((failures + 1))
    return
  fi
  ok "$name $("$name" --version 2>&1 | head -1 | cut -c1-40)"
}

need node 20 "install Node 20+ from https://nodejs.org (the sandbox needs Node 20)"
need python3 3 "install Python 3.9+ from https://www.python.org/downloads/"

# Rust: use rustup when it is available, because it can also install components.
if command -v cargo >/dev/null 2>&1; then
  ok "cargo $(cargo --version 2>&1 | cut -d' ' -f2)"
elif command -v rustup >/dev/null 2>&1; then
  note "cargo missing but rustup is present — installing the pinned toolchain"
  rustup toolchain install stable --profile minimal --component rustfmt,clippy && ok "cargo installed"
else
  bad "Rust is missing — install it with:"
  printf '        curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh\n'
  note "if static.rust-lang.org is unreachable (some sandboxes), conda-forge has a working toolchain:"
  printf '        micromamba create -y -p ./rustenv -c conda-forge rust binutils_linux-64\n'
  printf '        export PATH="$PWD/rustenv/bin:$PATH"\n'
  failures=$((failures + 1))
fi

# ---------------------------------------------------------------- 2. preflight
say "2/4  Preflight"
if python3 scripts/check-toolchain.py; then
  ok "preflight passed"
else
  note "preflight reported problems above; continuing so the rest can be checked"
fi

# ---------------------------------------------------------------- 3. dependencies
say "3/4  Dependencies"
# The host crate has none by design; this only warms the desktop crate's graph, and only if
# the shell is present.
if [ -f apps/desktop/src-tauri/Cargo.toml ] && command -v cargo >/dev/null 2>&1; then
  if (cd apps/desktop/src-tauri && cargo fetch --quiet 2>/dev/null); then
    ok "desktop dependency graph fetched"
  else
    note "could not fetch the desktop graph (offline?); the host crate still builds without it"
  fi
else
  note "desktop shell not present; nothing to fetch"
fi

# ---------------------------------------------------------------- 4. verify
say "4/4  Verification"
if bash scripts/verify-all.sh; then
  ok "every offline check passed"
else
  bad "verification failed — see the output above"
  failures=$((failures + 1))
fi

say "Done"
if [ "$failures" -gt 0 ]; then
  printf '  %s problem(s) remain.\n' "$failures"
  exit 1
fi
cat <<'EOF'
  This machine can build and verify Kavach.

  Next:
    ./scripts/build-local.sh --test      # Linux, macOS
    .\scripts\build-windows.ps1 -Test    # Windows

  What is NOT proven by this script: that the desktop shell compiles (it needs a system
  webview), that an installer exists, or that the Windows smoke test passes. Those need a
  machine with a webview, or the free self-hosted runner in .github/workflows.
EOF
