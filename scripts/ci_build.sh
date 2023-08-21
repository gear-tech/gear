#!/usr/bin/env bash

# Totally reproduces `CI::build` job locally.

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"

cd "$ROOT_DIR"

echo "Show: Versioning"
    ./scripts/gear.sh show

echo "Build: Gear"
    ./scripts/gear.sh build gear --locked --release

echo "Build fuzzer"
    ./scripts/gear.sh build fuzz --locked --release

echo "Check: Gear runtime imports"
    ./target/release/wasm-proc --check-runtime-imports target/release/wbuild/gear-runtime/gear_runtime.compact.wasm

echo "Check: Vara runtime imports"
    ./target/release/wasm-proc --check-runtime-imports target/release/wbuild/vara-runtime/vara_runtime.compact.wasm

echo "Test: Gear workspace"
    ./scripts/gear.sh test gear --exclude gclient --exclude gcli --release --locked

echo "Test: `gcli`"
    ./scripts/gear.sh test gcli --release --locked --retries 3

echo "Test: Client tests"
    ./scripts/gear.sh test client --release

echo "Test: gsdk tests"
    ./scripts/gear.sh test gsdk --release

echo "Test: Runtime benchmarks and benchmark tests work"
    cargo build -p gear-cli --release --features=runtime-benchmarks,runtime-benchmarks-checkers
    ./target/release/gear benchmark pallet --chain=dev --pallet=pallet_gear --steps=20 --extrinsic="*" --execution=wasm --wasm-execution=compiled --heap-pages=4096
    ./target/release/gear benchmark pallet --chain=dev --pallet=pallet_gear --extrinsic="check_all" --execution=wasm --wasm-execution=compiled --heap-pages=4096 --extra
    ./target/release/gear benchmark pallet --chain=dev --pallet=pallet_gear --extrinsic="check_lazy_pages_all" --execution=native --heap-pages=4096 --extra

echo "Test: Sys-calls Wasmi integrity"
    ./scripts/gear.sh test syscalls --release

echo "Test: `try-runtime` feature tests"
    cargo test -p "pallet-*" --features try-runtime --release --locked

echo "Test: Try runtime migrations"
    cargo build -p gear-cli --features try-runtime --release --locked
    ./target/release/gear try-runtime --runtime ./target/release/wbuild/gear-runtime/gear_runtime.wasm on-runtime-upgrade --checks live --uri wss://rpc-private-testnet.gear-tech.io:443
