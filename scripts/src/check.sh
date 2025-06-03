#!/usr/bin/env sh

check_usage() {
  cat << EOF

  Usage:
    ./gear.sh check <FLAG>
    ./gear.sh check <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help       show help message and exit

  Subcommands:
    help             show help message and exit

    gear             check gear workspace compile
    runtime_imports  check runtime imports against the whitelist

EOF
}

gear_check() {
  echo "  >> Check workspace"
  __GEAR_WASM_BUILDER_NO_BUILD=1 SKIP_WASM_BUILD=1 cargo check --workspace "$@"

  echo "  >> Check crates that use 'cfg(fuzz)"
  RUSTFLAGS="--cfg fuzz" cargo check "$@" -p gear-common -p vara-runtime -p runtime-fuzzer -p runtime-fuzzer-fuzz
}

runtime_imports() {
    if [ ! -f target/debug/wasm-proc ]; then
        cargo build -p wasm-proc
    fi

    if [ ! -f target/debug/wbuild/vara-runtime/vara_runtime.wasm ]; then
        cargo build -p vara-runtime
    fi
    ./target/debug/wasm-proc --check-runtime-imports target/debug/wbuild/vara-runtime/vara_runtime.wasm
}
