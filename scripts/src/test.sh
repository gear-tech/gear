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
    gsdk           run gsdk package tests
    gcli           run gcli package tests
    pallet         run pallet-gear tests
    client         run client tests via gclient
    fuzz           run fuzzer with a fuzz target
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear
    docs           run doc tests
    validators     run validator checks

EOF
}

test_run_node() {
  $EXE_RUNNER "$TARGET_DIR/release/gear$EXE_EXTENSION" "$@"
}

workspace_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz "$@" --no-fail-fast
  else
    cargo nextest run --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz "$@" --profile ci --no-fail-fast
  fi
}

gsdk_test() {
  $CARGO test -p gsdk
  $CARGO test -p gsdk --features vara-testing
}

gcli_test() {
  cargo nextest run -p gcli --profile ci --no-fail-fast "$@"
  cargo nextest run -p gcli --features vara-testing --profile ci --no-fail-fast "$@"
}

pallet_test() {
  cargo test -p pallet-gear "$@"
  cargo test -p pallet-gear-debug "$@"
  cargo test -p pallet-gear-payment "$@"
  cargo test -p pallet-gear-messenger "$@"
  cargo test -p pallet-gear-gas "$@"
}

client_tests() {
  $CARGO nextest run -p gclient --no-fail-fast "$@"
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

  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 cargo test --doc --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz --manifest-path="$MANIFEST" -- "$@"
}
