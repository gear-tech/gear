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
  SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo +nightly clippy --workspace "$@" --exclude gear-runtime --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz -- --no-deps -D warnings -A clippy::items-after-test-module
  SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 cargo +nightly clippy -p runtime-fuzzer -p runtime-fuzzer-fuzz -p gear-runtime --all-features -- --no-deps -D warnings -A clippy::items-after-test-module
}

# $1 - ROOT DIR
examples_clippy() {
  cd "$1"/examples
  SKIP_WASM_BUILD=1 cargo +nightly hack clippy --workspace --release -- --no-deps \
	  -A clippy::stable_sort_primitive \
    -D warnings
  cd "$1"
}
