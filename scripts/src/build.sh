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
    examples       build gear program examples
    wasm-proc      build wasm-proc util
    examples-proc  process built examples via wasm-proc
    node           build node

EOF
}

gear_build() {
  cargo build --workspace "$@"
}

node_build() {
  cargo build -p gear-node "$@"
}

wasm_proc_build() {
  cargo build -p wasm-proc --release
}

# $1 = TARGET DIR
examples_proc() {
  "$1"/release/wasm-proc -p "$1"/wasm32-unknown-unknown/release/*.wasm
}

# $1 = ROOT DIR, $2 = TARGET DIR
examples_build() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"
  shift
  shift
  cd "$ROOT_DIR"/examples
  CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release --workspace "$@"
  cd "$ROOT_DIR"
}
