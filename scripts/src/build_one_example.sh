#!/usr/bin/env sh

# Go into gear source root dir and run:
# $ script/src/build_one_example.sh ./examples/your_test_name

set -e

ROOT_DIR="$(pwd)"
TARGET_DIR="$ROOT_DIR/target"

cd "$ROOT_DIR"/$1
CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release
cd -

"$TARGET_DIR"/release/wasm-proc -p "$TARGET_DIR"/wasm32-unknown-unknown/release/*.wasm

