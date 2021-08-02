#!/usr/bin/env bash

set -e
cd "$(dirname ${BASH_SOURCE[0]})/.."

./scripts/build-wasm.sh
cargo run --package gear-test --release -- test/code/*.yaml
