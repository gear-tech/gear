#!/usr/bin/env sh

test_usage() {
  cat << EOF

  Usage:
    ./gear.sh test <FLAG>
    ./gear.sh test <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           run workspace tests
    js             run metadata js tests
    gtest          run gear-test testing tool,
                   you can specify yaml list to run using yamls="path/to/yaml1 path/to/yaml2 ..." argument
    rtest          run node runtime testing tool
    pallet         run pallet-gear tests
    runtime-upgrade run runtime-upgrade test for queue processing

EOF
}

workspace_test() {
  cargo nextest run --workspace "$@"
}

# $1 - ROOT DIR
js_test() {
  node "$1"/utils/wasm-proc/metadata-js/test.js
}

gtest_debug() {
  ROOT_DIR="$1"
  shift

  if [ -n "$1" ]
  then
    has_yamls=$(echo "$1" | grep "yamls=" || true)
  else
    has_yamls=""
  fi

  if  [ -n "$has_yamls" ]
  then
    if ! hash perl 2>/dev/null
    then
      echo "Can not parse yamls without \"perl\" installed =("
      exit 1
    fi

    YAMLS=$(echo $1 | perl -ne 'print $1 if /yamls=(.*)/s')
    shift
  fi

  if [ -z "$YAMLS" ]
  then
    YAMLS="$ROOT_DIR/gear-test/spec/*.yaml $ROOT_DIR/gear-test/spec_no_runtime/*.yaml"
  fi

  cargo run --package gear-test $CARGO_FLAGS -- $YAMLS "$@"
}

gtest() {
  CARGO_FLAGS="--release"
  gtest_debug "$1"
}

# $1 - ROOT DIR
rtest_debug() {
  cargo run --package gear-node $CARGO_FLAGS -- runtime-spec-tests "$1"/gear-test/spec/*.yaml -l0
}

rtest() {
  CARGO_FLAGS="--release"
  rtest_debug $1
}

pallet_test() {
  cargo test -p pallet-gear "$@"
}

# $1 - ROOT DIR
runtime_upgrade_test() {
  TEST_SCRIPT_PATH="$1/scripts/test-utils"

  RUNTIME_PATH="$1/scripts/test-utils/gear_runtime.compact.compressed.wasm"
  DEMO_PING_PATH="$1/target/wasm32-unknown-unknown/release/demo_ping.opt.wasm"

  # Run node
  RUST_LOG="pallet_gear=debug,runtime::gear::hooks=debug" cargo run --package gear-node --release -- --dev --tmp --unsafe-ws-external --unsafe-rpc-external --rpc-methods Unsafe --rpc-cors all & sleep 2

  # Change dir to the js script dir
  cd "$TEST_SCRIPT_PATH"

  # Run test
  npm test "$RUNTIME_PATH" "$DEMO_PING_PATH"

  # Killing node process added in js script
}
