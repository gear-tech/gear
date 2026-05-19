#!/usr/bin/env bash

clippy_usage() {
  cat << EOF

  Usage:
    ./gear.sh clippy <FLAG>
    ./gear.sh clippy <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           check gear workspace for clippy errors
    examples       check gear examples for clippy errors

EOF
}

gear_clippy() {
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 cargo clippy --workspace "$@" -- --no-deps -D warnings
}

examples_clippy() {
  # in case of `--all-targets` we can check tests, benches and so on
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 cargo clippy -p "demo-*" -p test-syscalls --no-default-features "$@" -- --no-deps -D warnings

  # find crates that use "gear-wasm-builder"
  mapfile -t examples < <(
    cargo metadata --no-deps --format-version=1 |
    jq -r '.workspace_root as $root | .packages.[] | select(.manifest_path | startswith($root + "/sdk/examples/")) | select(any(.dependencies.[]; .name == "gear-wasm-builder")) | "-p=" + .name'
  )
  # clippy will try to link "test" crate which is not available for "wasm32v1-none" target
  mapfile -t filtered_args < <(printf "%s\n" "${@}" | grep -v "all-targets")
  __GEAR_WASM_BUILDER_NO_BUILD=1 \
  SKIP_WASM_BUILD=1 \
  cargo clippy "${examples[@]}" "${filtered_args[@]}" --no-default-features --target=wasm32v1-none -- -D warnings
}

no_std_clippy() {
  mapfile -t no_std < <(
    cargo metadata --no-deps --format-version=1 |
    jq -r '.workspace_members as $members | .packages.[] | . as $pkg | select($members | index($pkg.id)) | select(.features | index("std")) | select(.name != "sc-executor") | "-p=" + .name'
  )
  RUSTFLAGS="--cfg=substrate_runtime" \
  __GEAR_WASM_BUILDER_NO_BUILD=1 \
  SKIP_WASM_BUILD=1 \
  cargo clippy "${no_std[@]}" "$@" --no-default-features --target=wasm32v1-none -- --no-deps -D warnings
}
