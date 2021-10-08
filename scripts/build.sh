#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

TARGET_DIR="$ROOT_DIR/target/examples"

BUILD_MODE=""

# Set build to release if needs
if [ "$2" == "release" ] ; then
    BUILD_MODE="--release"
fi

gear_build() {
    cargo build --workspace $BUILD_MODE
}

# Get newline-separated list of all workspace members in `$1/Cargo.toml`
get_members() {
  tr -d "\n" < "$1/Cargo.toml" |
    sed -n -e 's/.*members[[:space:]]*=[[:space:]]*\[\([^]]*\)\].*/\1/p' |
    sed -n -e 's/,/ /gp' |
    sed -n -e 's/"\([^"]*\)"/\1/gp'
}

wasm_proc_build() {
    cargo build -p wasm-proc --release
}

examples_proc() {
    rm -f $TARGET_DIR/wasm32-unknown-unknown/release/*.opt.wasm
    rm -f $TARGET_DIR/wasm32-unknown-unknown/release/*.meta.wasm
    wasm_proc_build
    $ROOT_DIR/target/release/wasm-proc -p $TARGET_DIR/wasm32-unknown-unknown/release/*.wasm
}

examples_build() {
    # For each entry in Cargo.toml workspace members:
    for entry in $(get_members $ROOT_DIR/examples); do
        # Quotes around `$entry` are not used intentionally to support globs in entry syntax, e.g. "member/*"
        for member in "$ROOT_DIR"/examples/$entry; do
            cd "$member"
            CARGO_TARGET_DIR=$TARGET_DIR cargo +nightly build --release
        done
    done
    cd $ROOT_DIR
}

node_build() {
    cargo build -p node $BUILD_MODE
}

case "$1" in
    all)
            gear_build
            examples_build
            examples_proc
            ;;
    gear)
            gear_build
            ;;
    examples)
            examples_build
            examples_proc
            ;;
    node)
            node_build
            ;;
    wasm-proc)
            wasm_proc_build
            examples_proc
            ;;
esac
