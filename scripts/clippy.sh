#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

BUILD_MODE=""

# Set build to release if needs
if [ "$2" == "release" ] ; then
    BUILD_MODE="--release"
fi

gear_clippy() {
    cd $ROOT_DIR
    cargo +nightly clippy --workspace $BUILD_MODE --all-features --no-deps -- -D warnings
}

examples_clippy() {
    cd $ROOT_DIR/examples
    cargo +nightly clippy --workspace $BUILD_MODE --no-deps -- \
        -A clippy::missing_safety_doc \
		-A clippy::stable_sort_primitive \
		-D warnings
}

case "$1" in
    all)
            gear_clippy
            examples_clippy
            ;;
    gear)
            gear_clippy
            ;;
    examples)
            examples_clippy
            ;;
esac
