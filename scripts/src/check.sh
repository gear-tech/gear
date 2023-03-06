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
    examples       check gear program examples compile

EOF
}

gear_check() {
  SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo check --workspace "$@"
}

# $1 = ROOT DIR, $2 = TARGET DIR
examples_check() {
  cd "$1"/examples
  SKIP_WASM_BUILD=1 CARGO_TARGET_DIR="$2" cargo +nightly hack check --release --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz
  cd "$1"
}
