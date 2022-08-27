#!/usr/bin/env sh

. $(dirname "$SELF")/src/common.sh

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
    client-weights run client weight test for infinite loop demo execution
    fuzz           run fuzzer with a fuzz target

EOF
}

workspace_test() {
  cargo nextest run --workspace "$@" --profile ci
}

# $1 - ROOT DIR
js_test() {
  node "$1"/utils/wasm-proc/metadata-js/test.js
}

# $1 - ROOT DIR
# $2 - yamls list (optional)
gtest() {
  ROOT_DIR="$1"
  shift

  YAMLS=$(parse_yamls_list "$1")

  is_yamls_arg=$(echo "$1" | grep "yamls=" || true)
  if [ -n "$is_yamls_arg" ]
  then
    shift
  fi

  if [ -z "$YAMLS" ]
  then
    YAMLS="$ROOT_DIR/gear-test/spec/*.yaml $ROOT_DIR/gear-test/spec_no_runtime/*.yaml"
  fi

  $ROOT_DIR/target/release/gear-test $YAMLS "$@"
}

# $1 - ROOT DIR
# $2 - TARGET DIR
# $3 - yamls list (optional)
rtest() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"

  YAMLS=$(parse_yamls_list "$3")

  if [ -z "$YAMLS" ]
  then
    YAMLS="$ROOT_DIR/gear-test/spec/*.yaml"
  fi

  $ROOT_DIR/target/release/gear-node runtime-spec-tests $YAMLS -l0 --generate-junit "$TARGET_DIR"/runtime-test-junit.xml
}

pallet_test() {
  cargo test -p pallet-gear "$@"
  cargo test -p pallet-gear-debug "$@"
  cargo test -p pallet-gear-payment "$@"
  cargo test -p pallet-gear-messenger "$@"
  cargo test -p pallet-gear-program "$@"
  cargo test -p pallet-gear-gas "$@"
}

# $1 - ROOT DIR
runtime_upgrade_test() {
  ROOT_DIR="$1"
  TEST_SCRIPT_PATH="$ROOT_DIR/scripts/test-utils"
  RUNTIME_PATH="$ROOT_DIR/scripts/test-utils/gear_runtime.compact.compressed.wasm"
  DEMO_PING_PATH="$ROOT_DIR/target/wasm32-unknown-unknown/release/demo_ping.opt.wasm"

  # Run node
  RUST_LOG="pallet_gear=debug,runtime::gear=debug" $ROOT_DIR/target/release/gear-node \
  --dev --tmp --unsafe-ws-external --unsafe-rpc-external --rpc-methods Unsafe --rpc-cors all & sleep 7

  # Change dir to the js script dir
  cd "$TEST_SCRIPT_PATH"

  # Run test
  npm run upgrade "$RUNTIME_PATH" "$DEMO_PING_PATH"

  # Killing node process added in js script
}

# $1 - ROOT DIR
client_weights_test() {
  ROOT_DIR="$1"
  TEST_SCRIPT_PATH="$ROOT_DIR/scripts/test-utils"
  DEMO_LOOP_PATH="$ROOT_DIR/target/wasm32-unknown-unknown/release/demo_loop.opt.wasm"

  # Run node
  RUST_LOG="gear=debug,gwasm=debug" $ROOT_DIR/target/release/gear-node \
  --dev --tmp --unsafe-ws-external --unsafe-rpc-external --rpc-methods Unsafe --rpc-cors all & sleep 7

  # Change dir to the js script dir
  cd "$TEST_SCRIPT_PATH"

  # Run test
  npm run weights "$DEMO_LOOP_PATH"

  # Killing node process added in js script
}

run_fuzzer() {
  ROOT_DIR="$1"

  for i in "${@:2}"; do
    case $i in
      *_fuzz_target)
        TARGET="${i}"
        ;;
      *)
        FEATURES="$FEATURES ${i}"
        ;;
    esac
  done

  if [[ -z $TARGET ]]
  then
    TARGET="simple_fuzz_target"
  fi

  # Navigate to fuzzer dir
  cd $ROOT_DIR/utils/economic-checks

  # Run fuzzer
  RUST_LOG="essential,pallet_gear=debug,gear_core_processor::executor=debug,economic_checks=debug,gwasm=debug" \
  cargo fuzz run --release "$FEATURES" --sanitizer=none "$TARGET"
}
