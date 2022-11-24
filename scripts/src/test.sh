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
    client         run client tests via gclient
    fuzz           run fuzzer with a fuzz target
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear

EOF
}

workspace_test() {
  cargo nextest run --workspace "$@" --profile ci --no-fail-fast
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
# $3 - runtime str (gear / vara)
# $4 - yamls list (optional)
rtest() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"
  RUNTIME_STR="$3"

  YAMLS=$(parse_yamls_list "$4")

  if [ -z "$YAMLS" ]
  then
    YAMLS="$ROOT_DIR/gear-test/spec/*.yaml"
  fi

  $ROOT_DIR/target/release/gear runtime-spec-tests $YAMLS -l0 --runtime "$RUNTIME_STR" --generate-junit "$TARGET_DIR"/runtime-test-junit.xml
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
  RUST_LOG="pallet_gear=debug,gear::runtime=debug" $ROOT_DIR/target/release/gear \
  --dev --tmp --unsafe-ws-external --unsafe-rpc-external --rpc-methods Unsafe --rpc-cors all & sleep 3

  # Change dir to the js script dir
  cd "$TEST_SCRIPT_PATH"

  # Run test
  npm run upgrade "$RUNTIME_PATH" "$DEMO_PING_PATH"

  # Killing node process added in js script
}

client_tests() {
  ROOT_DIR="$1"

  if [ "$2" = "--run-node" ]; then
    # Run node
    RUST_LOG="pallet_gear=debug,gear::runtime=debug" $ROOT_DIR/target/release/gear \
      --dev --tmp --unsafe-ws-external --unsafe-rpc-external --rpc-methods Unsafe --rpc-cors all & sleep 3

    cargo test -p gclient -- --test-threads 1 || pkill -f 'gear |gear$' -9 | pkill -f 'gear |gear$' -9
  else
    cargo test -p gclient -- --test-threads 1
  fi
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

# TODO this is likely to be merged with `pallet_test` or `workspace_test` in #1802
syscalls_integrity_test() {
  cargo test -p pallet-gear check_syscalls_integrity --features runtime-benchmarks
}

doc_test() {
  cargo test --doc --workspace "$@"
}
