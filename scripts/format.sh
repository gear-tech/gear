#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

BUILD_MODE=""

# Set build to release if needs
if [ "$2" == "check" ] ; then
    BUILD_MODE="--check"
fi

gear_format() {
    cargo fmt --all --manifest-path $ROOT_DIR/Cargo.toml -- \
        --config=license_template_path="" \
        $BUILD_MODE
}

examples_format() {
    cargo fmt --all --manifest-path $ROOT_DIR/examples/Cargo.toml -- \
        --config=license_template_path="" \
        $BUILD_MODE
}

doc_format() {
    cargo +nightly fmt -p gstd -p gcore -p gstd-async -- \
        --config wrap_comments=true,format_code_in_doc_comments=true \
        $BUILD_MODE
}

case "$1" in
    all)
            gear_format
            examples_format
            doc_format
            ;;
    gear)
            gear_format
            ;;
    examples)
            examples_format
            ;;
    doc)
            doc_format
            ;;
esac
