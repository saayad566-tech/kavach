#!/usr/bin/env bash
# Build Kavach locally. No CI, no account, no cost.
#
# This is the 100% free path: everything below runs on the machine in front of you.
# It produces the same installers the CI job produces.
#
#   ./scripts/build-local.sh            # build for this platform
#   ./scripts/build-local.sh --test     # run the full offline suite first
#
# Linux and macOS. For Windows use scripts/build-windows.ps1.

set -euo pipefail
cd "$(dirname "$0")/.."

RUN_TESTS=0
for arg in "$@"; do
  case "$arg" in
    --test) RUN_TESTS=1 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

say "0/4  Toolchain preflight"
python3 scripts/check-toolchain.py || exit 1

command -v node  >/dev/null || { echo "node not found. Install Node 20+: https://nodejs.org"; exit 1; }

say "1/4  Offline verification"
if [ "$RUN_TESTS" -eq 1 ]; then
  cargo test --offline --workspace
  python3 scripts/check-desktop-config.py --verbose
  python3 scripts/check-ui-contract.py
  python3 scripts/check-free.py
  node scripts/verify-ui.mjs
else
  cargo test --offline --workspace --quiet
fi

say "2/4  Desktop dependencies"
# First run downloads the crate graph from crates.io. That is free; it is only bandwidth.
cd apps/desktop/src-tauri
cargo fetch

say "3/4  Building the application and bundles"
# --bundles is platform specific: deb/appimage on Linux, dmg/app on macOS.
case "$(uname -s)" in
  Linux)  BUNDLES="deb,appimage" ;;
  Darwin) BUNDLES="dmg,app" ;;
  *)      BUNDLES="" ;;
esac
if [ -n "$BUNDLES" ]; then
  npx --yes @tauri-apps/cli@2 build --bundles "$BUNDLES"
else
  npx --yes @tauri-apps/cli@2 build
fi

say "4/4  Artefacts"
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null || echo target)"
find "$TARGET_DIR" -path '*release/bundle*' -type f \( -name '*.deb' -o -name '*.AppImage' -o -name '*.dmg' -o -name '*.app' \) -print0 2>/dev/null \
  | xargs -0 -r sha256sum

echo
echo "Done. Everything above ran locally and cost nothing."
echo "No account, no certificate, no hosted service was involved."
