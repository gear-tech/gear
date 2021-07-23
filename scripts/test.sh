#!/bin/sh

set -e
./build-wasm.sh
cargo run --release -- json/*.json
