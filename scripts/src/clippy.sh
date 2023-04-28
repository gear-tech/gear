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
    examples       check gear program examples for clippy errors

EOF
}

gear_clippy() {
  # `nightly`` is used for the workspace as the clippy check is run with `--all-features`.
  SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo +nightly clippy --workspace "$@" --exclude gear-runtime --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz -- --no-deps -D warnings
  SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 cargo +nightly clippy -p runtime-fuzzer -p runtime-fuzzer-fuzz -p gear-runtime --all-features -- --no-deps -D warnings
}

examples_clippy() {
  SKIP_WASM_BUILD=1 cargo +nightly clippy --no-default-features -p "demo-*" "$@" -- --no-deps -D warnings
  SKIP_WASM_BUILD=1 cargo +nightly clippy -p "demo-*" "$@" -- --no-deps -D warnings
}
