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
    gear-test      build gear-test binary
    examples       build gear program examples,
                   you can specify yaml list to build coresponding examples
                   using yamls="path/to/yaml1 path/to/yaml2 ..." argument
    wasm-proc      build wasm-proc util
    examples-proc  process built examples via wasm-proc
    node           build node

EOF
}

gear_build() {
  cargo build --workspace --exclude economic-checks --exclude economic-checks-fuzz "$@"
}

gear_test_build() {
  cargo build -p gear-test "$@"
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

  YAMLS=$(parse_yamls_list "$1")

  is_yamls_arg=$(echo "$1" | grep "yamls=" || true)
  if [ -n "$is_yamls_arg" ]
  then
    shift
  fi

  if [ -z "$YAMLS" ]
  then
    cd "$ROOT_DIR"
    cargo +nightly build --release -p "demo-*"
    cd "$ROOT_DIR"/examples
    CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release --workspace "$@"
    cd "$ROOT_DIR"
  else
    # If there is specified yaml list, then parses yaml files and build
    # all examples which is used as deps inside yamls.
    for path in $(get_demo_list $ROOT_DIR $YAMLS)
    do
      cd $path
      CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release "$@"
      cd -
    done
  fi
}
