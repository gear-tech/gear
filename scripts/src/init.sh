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
  cd "$1"/utils/wasm-proc/metadata-js
  npm install
  cd "$1"/scripts/test-utils
  npm install
  cd "$1"/gear-test/src/js
  npm install
  cd "$1"
}

# $1 = ROOT_DIR
js_update() {
  cd "$1"/utils/wasm-proc/metadata-js
  npm update
  cd "$1"/scripts/test-utils
  npm update
  cd "$1"/gear-test/src/js
  npm update
  cd "$1"
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
