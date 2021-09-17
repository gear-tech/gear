#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

./scripts/fmt.sh

echo "*** Run tests"
cargo test --workspace
./scripts/meta-test.sh
