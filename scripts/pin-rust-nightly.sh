#!/bin/sh

if [ "$#" -ge 1 ]; then
    echo "Trying to switch to date `$1`, please check, that format is `yyyy-mm-dd`"
else
    echo "No date provided"
    exit 1
fi

pin_date=$1
os_name="$(uname)"

if [ "$os_name" = "Darwin" ]; then
    rustup toolchain install nightly-$pin_date --component llvm-tools-preview
    rustup target add wasm32-unknown-unknown --toolchain nightly-$pin_date
    rm -rf ~/.rustup/toolchains/nightly-aarch64-apple-darwin
    mv ~/.rustup/toolchains/nightly-$pin_date-aarch64-apple-darwin ~/.rustup/toolchains/nightly-aarch64-apple-darwin
elif [ "$os_name" = "Linux" ]; then
    rustup toolchain install nightly-$pin_date --component llvm-tools-preview
    rustup target add wasm32-unknown-unknown --toolchain nightly-$pin_date
    rm -rf ~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu
    mv ~/.rustup/toolchains/nightly-$pin_date-x86_64-unknown-linux-gnu ~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu
else
    echo "Unknown operating system"
fi
