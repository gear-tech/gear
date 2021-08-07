#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

./scripts/build-wasm.sh
cargo run --package gear-test --release -- test/code/*.yaml
