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
    gcli           run gcli package tests
    pallet         run pallet-gear tests
    client         run client tests via gclient
    fuzz           run fuzzer with a fuzz target
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear
    docs           run doc tests
    validators     run validator checks

EOF
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
  cargo +nightly fuzz run --release --sanitizer=none main -- -rss_limit_mb=8192
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
