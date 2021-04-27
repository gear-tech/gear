#!/bin/sh

cargo +nightly build --manifest-path=demo-ping/Cargo.toml --target=wasm32-unknown-unknown --release &&
cargo +nightly build --manifest-path=demo-fib/Cargo.toml --target=wasm32-unknown-unknown --release &&
cargo +nightly build --manifest-path=demo-sum/Cargo.toml --target=wasm32-unknown-unknown --release &&
cargo +nightly build --manifest-path=demo-vec/Cargo.toml --target=wasm32-unknown-unknown --release &&
cargo +nightly build --manifest-path=demo-chat/Cargo.toml --target=wasm32-unknown-unknown --release
cargo run json/*.json
