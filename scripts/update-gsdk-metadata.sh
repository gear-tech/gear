#!/usr/bin/env bash

# This script is used to update the generated code in the gsdk crate
# Also see `gsdk/HOW-TO-UPDATE.md` for more details

set -ex

cargo build --package vara-runtime --features debug-mode --release
cargo build --package gsdk-api-gen --release
touch gsdk/build.rs
GSDK_API_GEN=1 cargo build --package gsdk --release
cargo fmt
