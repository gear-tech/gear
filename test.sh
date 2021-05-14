#!/bin/sh
./build-wasm.sh
cargo run --release -- json/*.json
