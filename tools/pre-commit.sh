#!/bin/bash

set -e
cd "$(dirname "$(readlink -f "$0")")/.."

# Format
cargo fmt --all

# Run linter
cargo clippy -- -D warnings
cd gear-backend && cargo clippy -- -D warnings
cd ..
cd gear-runner  && cargo clippy -- -D warnings
cd ..

# Build
cargo build --features=strict

# Run tests
cargo test --features=strict
cd gear-backend && cargo test --all-features
cd ..
cd gear-runner && cargo test
cd ..
