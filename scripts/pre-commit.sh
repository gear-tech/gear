#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/.."

<<<<<<< HEAD
./scripts/fmt.sh
=======

>>>>>>> origin/master

echo "*** Run tests"
cargo test --workspace
./scripts/meta-test.sh
