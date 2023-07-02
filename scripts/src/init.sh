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

cargo_init() {
  if [ -z $CI ] ; then
    cargo install cargo-hack
    cargo install cargo-nextest
  elif [ "$RUNNER_OS" = "Linux" ] ; then
    curl -L "https://github.com/taiki-e/cargo-hack/releases/latest/download/cargo-hack-x86_64-unknown-linux-gnu.tar.gz" |
    tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin

    curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
  fi
}
