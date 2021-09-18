#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

echo "*** Run fmt"
cargo fmt --all
cargo fmt --all --manifest-path examples/Cargo.toml -- --config=license_template_path=""

echo "*** Run clippy"
# TODO: Spread clippy to `--workspace`
cargo clippy --package gear-core -- -D warnings
cargo clippy --package gear-core-backend -- -D warnings
cargo clippy --package gear-core-runner -- -D warnings
cargo clippy --package gstd -- -D warnings
cargo clippy --package gstd-async -- -D warnings

echo "*** Run tests"
cargo test --workspace
./scripts/meta-test.sh
