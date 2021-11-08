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
    benchmark      check benchmarks compile

EOF
}

gear_check() {
  cargo check --workspace "$@"
}

# $1 = ROOT DIR, $2 = TARGET DIR
examples_check() {
  cd "$1"/examples
  CARGO_TARGET_DIR="$2" cargo +nightly hack check --release --workspace
  cd "$1"
}

benchmark_check() {
  cargo check --features=runtime-benchmarks "$@" \
    -p gear-node \
    -p pallet-gear \
    -p gear-runtime
}
