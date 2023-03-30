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
    gcli           run gcli package tests
    js             run metadata js tests
    gtest          run gear-test testing tool,
                   you can specify yaml list to run using yamls="path/to/yaml1 path/to/yaml2 ..." argument
    rtest          run node runtime testing tool
    pallet         run pallet-gear tests
    client         run client tests via gclient
    fuzz           run fuzzer with a fuzz target
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear

EOF
}

test_run_node() {
  $EXE_RUNNER "$TARGET_DIR/release/gear$EXE_EXTENSION" "$@"
}

workspace_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test --workspace "$@" --no-fail-fast
  else
    cargo +nightly nextest run --workspace "$@" --profile ci --no-fail-fast
  fi
}

gcli_test() {
  cargo +nightly nextest run -p gcli "$@" --profile ci --no-fail-fast
  cargo +nightly nextest run -p gcli "$@" --features vara-testing --profile ci --no-fail-fast
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

  test_run_node runtime-spec-tests $YAMLS -l0 --runtime "$RUNTIME_STR" --generate-junit "$TARGET_DIR"/runtime-test-junit.xml
}

pallet_test() {
  cargo test -p pallet-gear "$@"
  cargo test -p pallet-gear-debug "$@"
  cargo test -p pallet-gear-payment "$@"
  cargo test -p pallet-gear-messenger "$@"
  cargo test -p pallet-gear-gas "$@"
}

client_tests() {
  RUST_TEST_THREADS=1 $CARGO test -p gclient
}

validators() {
    ROOT_DIR="$1"

    $ROOT_DIR/target/release/validator-checks "${@:2}"
}

run_fuzzer() {
  ROOT_DIR="$1"

  # Navigate to fuzzer dir
  cd $ROOT_DIR/utils/runtime-fuzzer

  # Run fuzzer
  RUST_LOG="debug,runtime_fuzzer_fuzz=debug,wasmi,libfuzzer_sys,node_fuzzer=debug,gear,pallet_gear,gear-core-processor,gear-backend-wasmi,gwasm'" \
  cargo fuzz run --release --sanitizer=none main -- -rss_limit_mb=8192
}

# TODO this is likely to be merged with `pallet_test` or `workspace_test` in #1802
syscalls_integrity_test() {
  cargo test -p pallet-gear check_syscalls_integrity --features runtime-benchmarks "$@"
}

doc_test() {
  MANIFEST="$1"
  shift

  cargo test --doc --workspace --manifest-path="$MANIFEST" -- "$@"
}
