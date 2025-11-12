#!/usr/bin/env bash

# This script is used to update the generated code in the gsdk crate
# Also see `gsdk/HOW-TO-UPDATE.md` for more details

set -ex

cargo build --package vara-runtime --features dev
cargo run --package gsdk-scale-gen -- \
  target/debug/wbuild/vara-runtime/vara_runtime.wasm \
  gsdk/vara_runtime.scale
