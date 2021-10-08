#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

TARGET_DIR="$ROOT_DIR/target/examples"
WASM_SOURCE="$TARGET_DIR/wasm32-unknown-unknown/release"

BUILD_MODE=""
VERBOSE=""

# Set build to release if needs
if [ "$2" == "release" ] ; then
    BUILD_MODE="--release"
fi

# Set logging level with verbose
if [ "$2" == "v" ] || [ "$3" == "v" ] ; then
    VERBOSE="-v"
fi

# Set logging level with verbose
if [ "$2" == "vv" ] || [ "$3" == "vv" ] ; then
    VERBOSE="-vv"
fi

gear_test() {
    cargo test --workspace $BUILD_MODE
}

standalone_test() {
    cargo test \
        -p tests-btree \
        -p tests-common \
        -p tests-distributor \
        -p tests-gas-limit \
        -p tests-node \
        $BUILD_MODE
}

js_test() {
    node $ROOT_DIR/utils/wasm-proc/metadata-js/test.js
}

gtest() {
    cargo run --package gear-test --release -- $ROOT_DIR/gtest/spec/*.yaml $VERBOSE
}

ntest() {
    cargo run --package gear-node --release -- runtests $ROOT_DIR/gtest/spec/*.yaml
}

benchmark_test() {
    cargo check --release --features=runtime-benchmarks
}

case "$1" in
    all)
            $ROOT_DIR/scripts/env.sh js
            $ROOT_DIR/scripts/build.sh examples

            gear_test
            benchmark_test
            js_test
            gtest
            ntest
            ;;
    gear)
            $ROOT_DIR/scripts/env.sh js
            $ROOT_DIR/scripts/build.sh wasm-proc
            cd $ROOT_DIR/examples/guestbook
            CARGO_TARGET_DIR=$TARGET_DIR cargo +nightly build --release
            cd $ROOT_DIR
            $ROOT_DIR/target/release/wasm-proc -p $WASM_SOURCE/guestbook.wasm

            gear_test
            ;;
    standalone)
            standalone_test
            ;;
    js)
            $ROOT_DIR/scripts/env.sh js
            $ROOT_DIR/scripts/build.sh wasm-proc
            cd $ROOT_DIR/examples/async
            CARGO_TARGET_DIR=$TARGET_DIR cargo +nightly build --release
            cd $ROOT_DIR/examples/meta
            CARGO_TARGET_DIR=$TARGET_DIR cargo +nightly build --release
            cd $ROOT_DIR
            $ROOT_DIR/target/release/wasm-proc -p $WASM_SOURCE/demo_meta.wasm $WASM_SOURCE/demo_async.wasm

            js_test
            ;;
    gtest)
            $ROOT_DIR/scripts/env.sh js
            $ROOT_DIR/scripts/build.sh examples

            gtest
            ;;
    ntest)
            $ROOT_DIR/scripts/env.sh js
            $ROOT_DIR/scripts/build.sh examples

            ntest
            ;;
    bench)
            benchmark_test
            ;;
esac
