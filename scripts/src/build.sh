#!/usr/bin/env sh

. $(dirname "$0")/src/common.sh

build_usage() {
  cat << EOF

  Usage: ./gear.sh build [subcommand] [RUST_FLAGS]

  Subcommands:
    -h, --help     show help message and exit

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
  cargo build -p node "$@"
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
  for entry in $(get_members "$1"/examples); do
    for member in "$1"/examples/$entry; do
      cd "$member"
      CARGO_TARGET_DIR="$2" cargo +nightly build --release
    done
  done
}
