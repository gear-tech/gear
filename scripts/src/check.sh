#!/usr/bin/env sh

check_usage() {
  cat << EOF

  Usage:
    ./gear.sh check <FLAG>
    ./gear.sh check <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           check gear workspace compile

EOF
}

gear_check() {
  echo "  >> Check workspace without crates that use runtime with 'fuzz' feature"
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo check --workspace "$@" --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz

  echo "  >> Check crates that use runtime with 'fuzz' feature"
  cargo check "$@" -p runtime-fuzzer -p runtime-fuzzer-fuzz
}
