#!/usr/bin/env bash

# This script is used to update the generated code in the gsdk crate
# Also see `gsdk/HOW-TO-UPDATE.md` for more details

set -ex

cargo build --package vara-runtime --features dev
cargo build --package gsdk-api-gen
touch gsdk/build.rs
GSDK_API_GEN=1 cargo build --package gsdk
cargo fmt
