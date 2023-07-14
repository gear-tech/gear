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

EOF
}

gear_clippy() {
  EXCLUDE_PACKAGES="--exclude gear-runtime --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz"
  INCLUDE_PACKAGES="-p gear-runtime -p runtime-fuzzer -p runtime-fuzzer-fuzz"
  # `nightly`` is used for the workspace as the clippy check is run with `--all-features`.
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo clippy --workspace "$@" $EXCLUDE_PACKAGES -- --no-deps -D warnings
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 cargo clippy $INCLUDE_PACKAGES --all-features -- --no-deps -D warnings
}
