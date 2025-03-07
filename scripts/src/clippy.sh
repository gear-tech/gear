#!/usr/bin/env sh

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
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy --workspace "$@" -- --no-deps -D warnings
}

examples_clippy() {
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy -p "demo-*" -p test-syscalls --no-default-features "$@" -- --no-deps -D warnings
}
