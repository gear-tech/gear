#!/usr/bin/env sh

set -e
cd "$(dirname "$0")/../examples"

cargo +nightly build --workspace --release
