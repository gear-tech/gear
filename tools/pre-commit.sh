#!/bin/bash

set -e
cd "$(dirname "$(readlink -f "$0")")/.."

# Format
cargo fmt --all

# Run linter
cargo clippy -- -D warnings

# Build & run tests
cargo build
cargo test
cargo build --features=debug
cargo test --features=debug
