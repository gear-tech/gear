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
    wat-examples   build wat-examples

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

# $1 = ROOT DIR, $2 = TARGET DIR
examples_build() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"
  shift
  shift

  cd "$ROOT_DIR"
  cargo +nightly build --release -p "demo-*" "$@"
  cd "$ROOT_DIR"/examples
  CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release --workspace "$@"
  cd "$ROOT_DIR"
}

wat_examples_build() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"/wat-examples
  WAT_DIR="$ROOT_DIR/examples/wat-examples"
  mkdir -p $TARGET_DIR
  for wat in `ls $WAT_DIR`; do
    target_name=$TARGET_DIR/$(basename $wat .wat).wasm
    wat2wasm $WAT_DIR/$wat -o $target_name;
    echo "Built OK: $WAT_DIR/$wat";
  done
}
