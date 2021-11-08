#!/usr/bin/env sh

init_usage() {
  cat << EOF

  Usage:
    ./gear.sh init <FLAG>
    ./gear.sh init <SUBCOMMAND>

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    wasm           update rustc and add wasm target
    js             install js packages via npm
    update-js      update js packages via npm
    cargo          install 'cargo-hack' extension for cargo

EOF
}

wasm_init() {
  if [ -z $CI_PROJECT_NAME ] ; then
    rustup update nightly
    rustup update stable
  fi

  rustup target add wasm32-unknown-unknown --toolchain nightly
}

# $1 = ROOT_DIR
js_init() {
  npm --prefix "$1"/utils/wasm-proc/metadata-js install
  npm --prefix "$1"/gtest/src/js install
}

# $1 = ROOT_DIR
js_update() {
  npm --prefix "$1"/utils/wasm-proc/metadata-js update
  npm --prefix "$1"/gtest/src/js update
}

cargo_init() {
  cargo install cargo-hack
}
