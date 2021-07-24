#!/usr/bin/env bash

set -e
cd "$(dirname ${BASH_SOURCE[0]})/.."

echo "*** Run fmt"
cargo fmt --all

echo "*** Run clippy"
# TODO: Spread clippy to `--workspace`
cargo clippy --package gear-core -- -D warnings
cargo clippy --package gear-core-backend -- -D warnings
cargo clippy --package gear-core-runner -- -D warnings
cargo clippy --package gstd -- -D warnings

echo "*** Run tests"
cargo test --workspace
