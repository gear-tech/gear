#!/bin/sh

cargo +nightly build --manifest-path=demo/Cargo.toml --target=wasm32-unknown-unknown --release
cargo run -- 1 ./demo/target/wasm32-unknown-unknown/release/demo1.wasm
