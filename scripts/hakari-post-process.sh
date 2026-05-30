#!/usr/bin/env bash

# Post-processes gear-workspace-hack/Cargo.toml after `cargo hakari generate`
# to wrap all dependencies in cfgs that exclude wasm32 and loom builds.
#
# This ensures that on wasm32 and loom builds, the workspace-hack crate has zero
# dependencies (just a #![no_std] stub), while on normal native builds it
# retains full feature unification for optimal compilation.
#
# Usage: ./scripts/hakari-post-process.sh
# Run after: cargo hakari generate

set -euo pipefail

WORKSPACE_HACK="utils/gear-workspace-hack/Cargo.toml"

if [ ! -f "$WORKSPACE_HACK" ]; then
    echo "Error: $WORKSPACE_HACK not found. Run from workspace root." >&2
    exit 1
fi

CFG="target.'cfg(not(any(target_arch = \"wasm32\", loom)))'"

# Replace:
# - previously wrapped target dependency headers -> current cfg wrapper
# - platform-specific target headers -> equivalent cfg(...) plus the cfg wrapper
# - [dependencies.X] -> [target.'cfg(...)'.dependencies.X] (manual section)
# - [build-dependencies.X] -> [target.'cfg(...)'.build-dependencies.X]
# - [dependencies]    -> [target.'cfg(...)'.dependencies]    (hakari section)
# - [build-dependencies] -> [target.'cfg(...)'.build-dependencies]
#
# Platform-specific sections are still wrapped because they otherwise leak the
# workspace-hack dependency graph into loom builds on native hosts.
# Idempotent: already-wrapped headers won't match the patterns.
sed -i.bak -E \
    -e "s/^\[target\.'cfg\(all\(not\(target_arch = \"wasm32\"\), not\(loom\)\)\)'\./[${CFG}./" \
    -e "s/^\[target\.'cfg\(not\(target_arch = \"wasm32\"\)\)'\./[${CFG}./" \
    -e "s/^\[target\.x86_64-unknown-linux-gnu\.(dependencies|build-dependencies)([^]]*)\](.*)$/[target.'cfg(all(target_arch = \"x86_64\", target_vendor = \"unknown\", target_os = \"linux\", target_env = \"gnu\", not(any(target_arch = \"wasm32\", loom))))'.\1\2]\3/" \
    -e "s/^\[target\.aarch64-unknown-linux-gnu\.(dependencies|build-dependencies)([^]]*)\](.*)$/[target.'cfg(all(target_arch = \"aarch64\", target_vendor = \"unknown\", target_os = \"linux\", target_env = \"gnu\", not(any(target_arch = \"wasm32\", loom))))'.\1\2]\3/" \
    -e "s/^\[target\.aarch64-apple-darwin\.(dependencies|build-dependencies)([^]]*)\](.*)$/[target.'cfg(all(target_arch = \"aarch64\", target_vendor = \"apple\", target_os = \"macos\", not(any(target_arch = \"wasm32\", loom))))'.\1\2]\3/" \
    -e "s/^\[dependencies\.([^]]+)\](.*)$/[${CFG}.dependencies.\1]\2/" \
    -e "s/^\[dependencies\](.*)$/[${CFG}.dependencies]\1/" \
    -e "s/^\[build-dependencies\.([^]]+)\](.*)$/[${CFG}.build-dependencies.\1]\2/" \
    -e "s/^\[build-dependencies\](.*)$/[${CFG}.build-dependencies]\1/" \
    "$WORKSPACE_HACK"

rm -f "${WORKSPACE_HACK}.bak"

echo "Post-processed $WORKSPACE_HACK: all deps wrapped in cfg(not(any(target_arch = \"wasm32\", loom)))"
