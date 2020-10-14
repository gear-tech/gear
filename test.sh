#!/bin/sh

cargo build --manifest-path=demo/Cargo.toml --target=wasm32-unknown-unknown --release
cargo run -- ./demo/target/wasm32-unknown-unknown/release/demo1.wasm
