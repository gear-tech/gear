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
  $CARGO build --workspace "$@" --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz
}

fuzzer_build() {
  $CARGO build "$@" -p runtime-fuzzer -p runtime-fuzzer-fuzz
}

node_build() {
  $CARGO build -p gear-cli "$@"
  for f in ./target/release/build/gear-runtime-*/output
  do
    echo "Processing $f file..."
    # take action on each file. $f store current file name
    cat $f
  done
}

wasm_proc_build() {
  $CARGO build -p wasm-proc "$@"
}

gear_replay_build() {
  cargo build -p gear-replay-cli "$@"
}

# $1 = TARGET DIR
examples_proc() {
  WASM_EXAMPLES_DIR="$1"/wasm32-unknown-unknown/release
  WASM_EXAMPLES_LIST=$(find $WASM_EXAMPLES_DIR -name "*.wasm" | tr '\n' ' ' | sed 's/ $//')
  "$1"/release/wasm-proc $WASM_EXAMPLES_LIST
}

# $1 = ROOT DIR
examples_build() {
  ROOT_DIR="$1"
  shift

  cd "$ROOT_DIR"
  $CARGO build -p "demo-*" -p test-syscalls "$@"
}
