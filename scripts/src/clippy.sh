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
  # TODO #3452: remove `-A clippy::needless_pass_by_ref_mut` on next rust update
  EXCLUDE_PACKAGES="--exclude vara-runtime --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz"
  INCLUDE_PACKAGES="-p vara-runtime -p runtime-fuzzer -p runtime-fuzzer-fuzz"

  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy --workspace "$@" $EXCLUDE_PACKAGES -- --no-deps -D warnings -A clippy::needless_pass_by_ref_mut
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy $INCLUDE_PACKAGES --all-features -- --no-deps -D warnings -A clippy::needless_pass_by_ref_mut
}

examples_clippy() {
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy -p "demo-*" -p test-syscalls --no-default-features "$@" -- --no-deps -D warnings -A clippy::needless_pass_by_ref_mut
}
