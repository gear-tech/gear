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
  npm --prefix "$1"/scripts/test-utils install
  npm --prefix "$1"/gear-test/src/js install
}

# $1 = ROOT_DIR
js_update() {
  npm --prefix "$1"/utils/wasm-proc/metadata-js update
  npm --prefix "$1"/scripts/test-utils update
  npm --prefix "$1"/gear-test/src/js update
}

cargo_init() {
  if [ -z $CI ]; then
    cargo install cargo-hack
    cargo install cargo-nextest
  else
    curl "https://api.github.com/repos/taiki-e/cargo-hack/releases/latest" |
    grep -wo "https.*x86_64-unknown-linux-gnu.tar.gz" |
    xargs curl -L |
    tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin

    curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
  fi
}
