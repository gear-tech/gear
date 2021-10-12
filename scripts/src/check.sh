#!/usr/bin/env sh

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

# Get newline-separated list of all workspace members in `$1/Cargo.toml`
get_members() {
  tr -d "\n" < "$1/Cargo.toml" |
    sed -n -e 's/.*members[[:space:]]*=[[:space:]]*\[\([^]]*\)\].*/\1/p' |
    sed -n -e 's/,/ /gp' |
    sed -n -e 's/"\([^"]*\)"/\1/gp'
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
