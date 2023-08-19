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
    fuzz-repr      run fuzzer reproduction test
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear
    docs           run doc tests
    validators     run validator checks
    time-consuming run time consuming tests
EOF
}

workspace_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz --no-fail-fast "$@"
  else
    cargo nextest run --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz --profile ci --no-fail-fast "$@"
  fi
}

gsdk_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p gsdk --no-fail-fast "$@"
    $CARGO test -p gsdk --no-fail-fast --features vara-testing "$@"
  else
    cargo nextest run -p gsdk --profile ci --no-fail-fast "$@"
    cargo nextest run -p gsdk --features vara-testing --profile ci --no-fail-fast "$@"
  fi
}

gcli_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p gcli --no-fail-fast "$@"
    $CARGO test -p gcli --features vara-testing --no-fail-fast "$@"
  else
    cargo nextest run -p gcli --profile ci --no-fail-fast "$@"
    cargo nextest run -p gcli --features vara-testing --profile ci --no-fail-fast "$@"
  fi
}

pallet_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p "pallet-*" --no-fail-fast "$@"
  else
    cargo nextest run -p "pallet-*" --profile ci --no-fail-fast "$@"
  fi
}

client_tests() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p gclient --no-fail-fast "$@"
  else
    cargo nextest run -p gclient --no-fail-fast "$@"
  fi
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
  RUST_LOG=debug,syscalls,gear_wasm_gen=trace,runtime_fuzzer=trace,gear_backend_common=trace \
  cargo fuzz run --release --sanitizer=none main -- -rss_limit_mb=8192 -max_len=35000000 -len_control=0
}

test_fuzzer_reproduction() {
  cargo nextest run -p runtime-fuzzer -E 'test(=test_fuzzer_reproduction)'
}

# TODO this is likely to be merged with `pallet_test` or `workspace_test` in #1802
syscalls_integrity_test() {
  $CARGO test -p pallet-gear check_syscalls_integrity --features runtime-benchmarks --no-fail-fast "$@"
}

doc_test() {
  MANIFEST="$1"
  shift

  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_GEAR_RUNTIME_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 $CARGO test --doc --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz --manifest-path="$MANIFEST" --no-fail-fast "$@"
}

time_consuming_tests() {
  $CARGO test -p demo-fungible-token --no-fail-fast "$@" -- --nocapture --ignored
}
