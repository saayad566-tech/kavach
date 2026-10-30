#!/usr/bin/env bash
# Build the distributable zip and prove it works.
#
# A zip is only "ready to use" if it has been unzipped somewhere else and built from there.
# This does exactly that: packs the tree, extracts it into a clean directory, and runs the
# whole offline suite against the extracted copy.
#
#   ./scripts/verify-zip.sh [output-dir]

set -uo pipefail
cd "$(dirname "$0")/.."
SRC="$(pwd)"
PARENT="$(dirname "$SRC")"
DIR_NAME="$(basename "$SRC")"
OUT_DIR="${1:-$SRC/../dist}"
NAME="kavach-ready-to-use"

mkdir -p "$OUT_DIR"
ZIP="$OUT_DIR/$NAME.zip"

echo "1/4  Packing"
rm -f "$ZIP"
# Pack from the parent so the archive has one top-level directory.
(
  cd "$PARENT" || exit 1
  zip -qr "$ZIP" "$DIR_NAME" \
    -x "$DIR_NAME/target/*" \
    -x "$DIR_NAME/.git/*" \
    -x '*__pycache__*' -x '*.pyc' -x '*.tmp'
) || { echo "     FAIL — zip failed"; exit 1; }
[ -f "$ZIP" ] || { echo "     FAIL — no archive was produced"; exit 1; }
echo "     $(du -h "$ZIP" | cut -f1)  $(basename "$ZIP")"

echo "2/4  Extracting to a clean directory"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
unzip -q "$ZIP" -d "$WORK"
EXTRACTED="$WORK/$DIR_NAME"
count=$(find "$EXTRACTED" -type f | wc -l | tr -d ' ')
echo "     $count files"

echo "3/4  Building from the extracted copy"
# The build directory must be inside the extraction, not a shared target dir.
if ! (cd "$EXTRACTED" && cargo test --offline --quiet >/dev/null 2>&1); then
  echo "     FAIL — the extracted copy does not build or test cleanly"
  echo "RESULT: FAIL"
  exit 1
fi
echo "     ok — the extracted copy builds and its tests pass"

echo "4/4  Full offline suite from the extracted copy"
if ! (cd "$EXTRACTED" && bash scripts/verify-all.sh >/dev/null 2>&1); then
  echo "     FAIL — verify-all failed on the extracted copy"
  echo "RESULT: FAIL"
  exit 1
fi
echo "     ok — every offline check passed from the extracted copy"

echo
echo "RESULT: PASS — $ZIP is ready to use"
echo "  sha256: $(sha256sum "$ZIP" | cut -d' ' -f1)"
