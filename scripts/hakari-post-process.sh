#!/usr/bin/env bash

# Post-processes gear-workspace-hack/Cargo.toml after `cargo hakari generate`
# to wrap all dependencies in cfg(not(target_arch = "wasm32")).
#
# This ensures that on wasm32 builds, the workspace-hack crate has zero
# dependencies (just a #![no_std] stub), while on native builds it retains
# full feature unification for optimal compilation.
#
# Usage: ./scripts/src/hakari-post-process.sh
# Run after: cargo hakari generate

set -euo pipefail

WORKSPACE_HACK="utils/gear-workspace-hack/Cargo.toml"

if [ ! -f "$WORKSPACE_HACK" ]; then
    echo "Error: $WORKSPACE_HACK not found. Run from workspace root." >&2
    exit 1
fi

CFG="target.'cfg(not(target_arch = \"wasm32\"))'"

# Replace:
# - [dependencies.X] -> [target.'cfg(...)'.dependencies.X] (manual section)
# - [dependencies]    -> [target.'cfg(...)'.dependencies]    (hakari section)
# - [build-dependencies] -> [target.'cfg(...)'.build-dependencies]
#
# Does NOT touch [target.TRIPLE.*] sections (already platform-specific).
# Idempotent: already-wrapped headers won't match the patterns.
sed -i.bak \
    -e "s/^\[dependencies\.\(.*\)\]$/[${CFG}.dependencies.\1]/" \
    -e "s/^\[dependencies\]$/[${CFG}.dependencies]/" \
    -e "s/^\[build-dependencies\]$/[${CFG}.build-dependencies]/" \
    "$WORKSPACE_HACK"

rm -f "${WORKSPACE_HACK}.bak"

echo "Post-processed $WORKSPACE_HACK: all deps wrapped in cfg(not(target_arch = \"wasm32\"))"
