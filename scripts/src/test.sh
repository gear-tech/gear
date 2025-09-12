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
  cargo nextest run --workspace \
    --exclude gclient --exclude gcli --exclude gsdk \
    --exclude runtime-fuzzer --exclude runtime-fuzzer-fuzz \
    --no-fail-fast "$@"
}

gsdk_test() {
  cargo nextest run -p gsdk --no-fail-fast "$@"
}

gcli_test() {
  cargo nextest run -p gcli --no-fail-fast "$@"
}

pallet_test() {
  cargo nextest run -p "pallet-*" --no-fail-fast "$@"
}

client_tests() {
  cargo nextest run -p gclient --no-fail-fast "$@"
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
  RUSTFLAGS="--cfg fuzz" RUST_LOG="$LOG_TARGETS" \
    cargo fuzz run --release --sanitizer=none runtime-fuzzer-fuzz $CORPUS_DIR -- -rss_limit_mb=$RSS_LIMIT_MB -max_len=$MAX_LEN -len_control=0
}

run_lazy_pages_fuzzer() {
  # Build/run fuzzer
  if [ -n "$LAZY_PAGES_FUZZER_ONLY_BUILD" ]
  then
    cargo build --release -p lazy-pages-fuzzer-runner
  else
    cargo run --release -p lazy-pages-fuzzer-runner -- run "$@"
  fi
}

run_fuzzer_tests() {
  # This includes property tests for runtime-fuzzer.
  RUSTFLAGS="--cfg fuzz" cargo nextest run -p runtime-fuzzer
}

# TODO this is likely to be merged with `pallet_test` or `workspace_test` in #1802
syscalls_integrity_test() {
  cargo test -p pallet-gear check_syscalls_integrity --features runtime-benchmarks --no-fail-fast "$@"
}

doc_test() {
  MANIFEST="$1"
  shift

  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 cargo test --doc --workspace --manifest-path="$MANIFEST" --no-fail-fast "$@"
}

time_consuming_tests() {
  # cargo test -p demo-fungible-token --no-fail-fast --release -- --nocapture --ignored
  cargo test -p gear-wasm-builder --no-fail-fast "$@" -- --nocapture --ignored
  LOOM_MAX_PREEMPTIONS=3 RUSTFLAGS="--cfg loom" cargo test -p gear-wasmer-cache --no-fail-fast --release -- --nocapture
}

ensure_binary() {
  BINARY="$1"
  HINT="$2"

  if ! command -v "${BINARY}" >/dev/null; then
    echo "You need \`${BINARY}\` program to run this script." >&2
    echo >&2
    echo "To install it, run following command:" >&2
    echo "> ${HINT}" >&2
    echo >&2
    exit 1
  fi
}
typo_tests() {
  ensure_binary "typos" "cargo install typos-cli"

  typos
}
