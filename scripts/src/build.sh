#!/usr/bin/env sh

build_usage() {
  cat << EOF

  Usage:
    ./gear.sh build <FLAG>
    ./gear.sh build <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           build gear workspace
    fuzz           build fuzzer crates
    examples       build gear program examples
    wasm-proc      build wasm-proc util
    examples-proc  process built examples via wasm-proc
    node           build node

EOF
}

gear_build() {
  $CARGO build --workspace "$@" --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz
}

fuzzer_build() {
  $CARGO +nightly build "$@" -p runtime-fuzzer -p runtime-fuzzer-fuzz
}

gear_test_build() {
  $CARGO build -p gear-test "$@"
}

node_build() {
  $CARGO build -p gear-cli "$@"
}

wasm_proc_build() {
  cargo build -p wasm-proc --release "$@"
}

# $1 = TARGET DIR
examples_proc() {
  "$1"/release/wasm-proc "$1"/wasm32-unknown-unknown/release/*.wasm
}

examples_build() {
  cargo build --release --no-default-features -p "demo-*" "$@"
}
