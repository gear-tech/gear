#!/usr/bin/env sh

. $(dirname "$0")/src/common.sh

check_usage() {
   cat << HEREDOC

   Usage: ./gear.sh check [subcommand] [RUST_FLAGS]

   Subcommands:
     -h, --help     show help message and exit

     gear           check gear workspace compile
     examples       check gear program examples compile
     benchmark      check benchmarks compile

HEREDOC
}

gear_check() {
    cargo check --workspace "$@"
}

# $1 = ROOT DIR, $2 = TARGET DIR
examples_check() {
    for entry in $(get_members $1/examples); do
        for member in "$1"/examples/$entry; do
            cd "$member"
            CARGO_TARGET_DIR="$2" cargo +nightly check --release
        done
    done
}

benchmark_check() {
    cargo check --features=runtime-benchmarks "$@" \
        -p gear-node \
        -p pallet-gear \
        -p gear-runtime
}
