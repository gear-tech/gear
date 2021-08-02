#!/usr/bin/env bash

set -e
cd "$(dirname ${BASH_SOURCE[0]})/../examples"

cargo +nightly build --workspace --release
