#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

./scripts/build-wasm.sh
rm -f ./examples/target/wasm32-unknown-unknown/release/*.opt.wasm
rm -f ./examples/target/wasm32-unknown-unknown/release/*.meta.wasm

./target/release/wasm-proc -p ./examples/target/wasm32-unknown-unknown/release/*.wasm
cargo run --package gear-test --release -- gtest/spec/*.yaml
