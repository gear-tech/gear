#!/usr/bin/env sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/../../.." && pwd)

SOURCE="$ROOT_DIR/ethexe/contracts/out/BatchMulticall.sol/BatchMulticall.json"
TARGET="$ROOT_DIR/ethexe/node-loader/BatchMulticall.json"

if [ ! -f "$SOURCE" ]; then
  echo "Source artifact not found: $SOURCE"
  echo "Run 'forge build' in ethexe/contracts first."
  exit 1
fi

if [ "${1:-}" = "--check" ]; then
  if cmp -s "$SOURCE" "$TARGET"; then
    echo "BatchMulticall.json is up to date"
    exit 0
  fi

  echo "BatchMulticall.json is outdated. Run: ./ethexe/node-loader/scripts/update-batch-multicall-json.sh"
  exit 1
fi

cp "$SOURCE" "$TARGET"
echo "Updated $TARGET"
