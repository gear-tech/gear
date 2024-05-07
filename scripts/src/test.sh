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
    fuzz           run fuzzer
                   The scripts accepts a path to corpus dir as a first param,
                   and a "wlogs" flag to enable logs while fuzzing.
    fuzz-repr      run fuzzer reproduction test
    syscalls       run syscalls integrity test in benchmarking module of pallet-gear
    docs           run doc tests
    validators     run validator checks
    time-consuming run time consuming tests
    typos          run typo tests
EOF
}

workspace_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test --workspace \
      --exclude gclient --exclude gcli --exclude gsdk \
      --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz \
      --no-fail-fast "$@"
  else
    cargo nextest run --workspace \
      --exclude gclient --exclude gcli --exclude gsdk \
      --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz \
      --profile ci --no-fail-fast "$@"
  fi
}

gsdk_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p gsdk --no-fail-fast "$@"
  else
    cargo nextest run -p gsdk --profile ci --no-fail-fast "$@"
  fi
}

gcli_test() {
  if [ "$CARGO" = "cargo xwin" ]; then
    $CARGO test -p gcli --no-fail-fast "$@"
  else
    cargo nextest run -p gcli --profile ci --no-fail-fast "$@"
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

    $ROOT_DIR/target/debug/validator-checks "${@:2}"
}

run_fuzzer() {
  . $(dirname "$SELF")/fuzzer_consts.sh

  ROOT_DIR="$1"
  CORPUS_DIR="$2"
  # Navigate to fuzzer dir
  cd $ROOT_DIR/utils/runtime-fuzzer

  if [ "$3" = "wlogs" ]; then
    LOG_TARGETS="debug,syscalls,runtime::sandbox=trace,gear_wasm_gen=trace,runtime_fuzzer=trace,gear_core_backend=trace"
  else
    LOG_TARGETS="off"
  fi

  # Run fuzzer
  RUST_LOG="$LOG_TARGETS" cargo fuzz run --release --sanitizer=none main $CORPUS_DIR -- -rss_limit_mb=$RSS_LIMIT_MB -max_len=$MAX_LEN -len_control=0
}

run_fuzzer_tests() {
  # This includes property tests for runtime-fuzzer.
  cargo nextest run -p runtime-fuzzer
}

# TODO this is likely to be merged with `pallet_test` or `workspace_test` in #1802
syscalls_integrity_test() {
  $CARGO test -p pallet-gear check_syscalls_integrity --features runtime-benchmarks --no-fail-fast "$@"
}

doc_test() {
  MANIFEST="$1"
  shift

  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 SKIP_VARA_RUNTIME_WASM_BUILD=1 $CARGO test --doc --workspace --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz --manifest-path="$MANIFEST" --no-fail-fast "$@"
}

time_consuming_tests() {
  $CARGO test -p demo-fungible-token --no-fail-fast --release -- --nocapture --ignored
  $CARGO test -p gear-wasm-builder --no-fail-fast "$@" -- --nocapture --ignored
}

typo_tests() {
  readonly COMMAND="typos"
  readonly VERSION='typos-cli 1.20.3'

  # Install typos-cli if not exist or outdated.
  if ! [ -x "$(command -v ${COMMAND})" ] || [ "$($COMMAND --version)" != "$VERSION" ]; then
    cargo install typos-cli
  fi

  typos
}
