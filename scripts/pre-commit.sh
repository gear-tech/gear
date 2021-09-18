#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

echo "*** Run tests"
cargo test --workspace
./scripts/meta-test.sh
