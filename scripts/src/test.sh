#!/usr/bin/env sh

test_usage() {
   cat << HEREDOC

   Usage: ./gear.sh test [subcommand]

   Subcommands:
     -h, --help     show help message and exit

     gear           run workspace tests
     js             run metadata js tests
     gtest          run gtest testing tool
     ntest          run node testsuite

HEREDOC
}

workspace_test() {
    cargo test --workspace $@
}

# $1 - ROOT DIR
js_test() {
    node $1/utils/wasm-proc/metadata-js/test.js
}

gtest() {
    ROOT_DIR=$1
    shift

    cargo run --package gear-test --release -- $ROOT_DIR/gtest/spec/*.yaml $@
}

# $1 - ROOT DIR
ntest() {
    cargo run --package gear-node --release -- runtests $1/gtest/spec/*.yaml
}
