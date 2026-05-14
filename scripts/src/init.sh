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

  rustup target add wasm32v1-none --toolchain stable
  rustup target add wasm32v1-none --toolchain nightly
}

cargo_init() {
  if [ -z $CI ] ; then
    cargo install cargo-hack
    cargo install --locked cargo-nextest
  elif [ "$RUNNER_OS" = "Linux" ] && [[ "$(uname -m)" =~ ^(x86_64|amd64)$ ]]; then
    curl -L "https://github.com/taiki-e/cargo-hack/releases/latest/download/cargo-hack-x86_64-unknown-linux-gnu.tar.gz" |
    tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin

    curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
  elif [ "$RUNNER_OS" = "Linux" ] && [[ "$(uname -m)" =~ ^(aarch64|arm64)$ ]]; then
    curl -L "https://github.com/taiki-e/cargo-hack/releases/latest/download/cargo-hack-aarch64-unknown-linux-gnu.tar.gz" |
    tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin

    curl -LsSf https://get.nexte.st/latest/linux-arm | tar zxf - -C ${CARGO_HOME:-~/.cargo}/bin
  else
    echo "Unsupported OS or architecture for cargo-hack and cargo-nextest installation."
  fi
}
