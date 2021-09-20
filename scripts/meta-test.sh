#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

echo "*** Build demo_meta wasm"
cd "examples/meta"
cargo +nightly-2021-09-14-aarch64-apple-darwin build --release

echo "*** Run wasm-proc"
cd "../../utils/wasm-proc"
cargo build --release
../../target/release/wasm-proc --path ../../examples/target/wasm32-unknown-unknown/release/demo_meta.wasm

echo "*** Run js test"
cd "metadata-js"
npm install
node ./test.js
