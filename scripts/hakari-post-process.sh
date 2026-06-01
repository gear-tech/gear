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
X86_64_LINUX_CFG="target.'cfg(all(target_arch = \"x86_64\", target_vendor = \"unknown\", target_os = \"linux\", target_env = \"gnu\", not(any(target_arch = \"wasm32\", loom))))'"
AARCH64_LINUX_CFG="target.'cfg(all(target_arch = \"aarch64\", target_vendor = \"unknown\", target_os = \"linux\", target_env = \"gnu\", not(any(target_arch = \"wasm32\", loom))))'"
AARCH64_MACOS_CFG="target.'cfg(all(target_arch = \"aarch64\", target_vendor = \"apple\", target_os = \"macos\", not(any(target_arch = \"wasm32\", loom))))'"

# Hakari currently emits only top-level dependency sections and exact platform
# target sections. Wrap those sections so the workspace-hack graph is absent
# from wasm32 and loom builds.
sed -i.bak -E \
    -e "s/^\[(dependencies|build-dependencies)\]$/[${CFG}.\1]/" \
    -e "s/^\[target\.x86_64-unknown-linux-gnu\.(dependencies|build-dependencies)\]$/[${X86_64_LINUX_CFG}.\1]/" \
    -e "s/^\[target\.aarch64-unknown-linux-gnu\.(dependencies|build-dependencies)\]$/[${AARCH64_LINUX_CFG}.\1]/" \
    -e "s/^\[target\.aarch64-apple-darwin\.(dependencies|build-dependencies)\]$/[${AARCH64_MACOS_CFG}.\1]/" \
    "$WORKSPACE_HACK"

rm -f "${WORKSPACE_HACK}.bak"

echo "Post-processed $WORKSPACE_HACK: generated deps/build-deps wrapped for wasm32/loom"
