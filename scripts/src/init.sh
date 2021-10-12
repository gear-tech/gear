#!/usr/bin/env sh

init_usage() {
   cat << HEREDOC

   Usage: ./gear.sh init [subcommand]

   Subcommands:
     -h, --help     show help message and exit

     wasm           update rustc and add wasm target
     js             install and update js packages via npm

HEREDOC
}

wasm_init() {
    if [ -z $CI_PROJECT_NAME ] ; then
        rustup update nightly
        rustup update stable
    fi

    rustup target add wasm32-unknown-unknown --toolchain nightly
}

js_init() {
    npm --prefix "$ROOT_DIR"/utils/wasm-proc/metadata-js install
    npm --prefix "$ROOT_DIR"/utils/wasm-proc/metadata-js update
    npm --prefix "$ROOT_DIR"/gtest/src/js install
    npm --prefix "$ROOT_DIR"/gtest/src/js update
}
