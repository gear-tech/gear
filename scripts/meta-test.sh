#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

echo "*** Build demo_meta wasm"
cd "examples/meta"
cargo +nightly build --release

echo "*** Run wasm-proc"
cd "../../utils/wasm-proc"
cargo build --release
../../target/release/wasm-proc --path ../../examples/target/wasm32-unknown-unknown/release/demo_meta.wasm
../../target/release/wasm-proc --path ../../examples/target/wasm32-unknown-unknown/release/demo_async.wasm

echo "*** Run js test"
cd "metadata-js"
npm install
node ./test.js
