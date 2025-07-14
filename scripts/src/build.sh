#!/usr/bin/env sh

. $(dirname "$SELF")/src/common.sh

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
    examples       build gear examples
    wasm-proc      build wasm-proc util
    examples-proc  process built examples via wasm-proc
    node           build node

EOF
}

gear_build() {
  cargo build --workspace "$@"
}

fuzzer_build() {
  RUSTFLAGS="--cfg fuzz" cargo build "$@" -p runtime-fuzzer -p runtime-fuzzer-fuzz
}

node_build() {
  cargo build -p gear-cli "$@"
}

wasm_proc_build() {
  cargo build -p wasm-proc "$@"
}

gear_replay_build() {
  cargo build -p gear-replay-cli "$@"
}

# $1 = TARGET DIR
examples_proc() {
  WASM_EXAMPLES_DIR="$1"/wasm32-gear/release
  WASM_EXAMPLES_LIST=$(find $WASM_EXAMPLES_DIR -name "*.wasm" | tr '\n' ' ' | sed 's/ $//')
  "$1"/release/wasm-proc $WASM_EXAMPLES_LIST
}

# $1 = ROOT DIR
examples_build() {
  ROOT_DIR="$1"
  shift

  cd "$ROOT_DIR"
  cargo build -p "demo-*" -p test-syscalls "$@"
}
