#!/bin/sh

set -e

if [ "$#" -ge 1 ]; then
    echo "Trying to switch to date $1, please check, that format is yyyy-mm-dd"
else
    echo "No date provided"
    exit 1
fi

pin_date=$1
os_name="$(uname)"

if [ "$os_name" = "Darwin" ]; then
    suffix=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")
    rustup toolchain install nightly-$pin_date --component llvm-tools-preview
    rustup target add wasm32-unknown-unknown --toolchain nightly-$pin_date
    rm -rf ~/.rustup/toolchains/nightly-$suffix
    mv ~/.rustup/toolchains/nightly-$pin_date-$suffix ~/.rustup/toolchains/nightly-$suffix
elif [ "$os_name" = "Linux" ]; then
    rustup toolchain install nightly-$pin_date --component llvm-tools-preview
    rustup target add wasm32-unknown-unknown --toolchain nightly-$pin_date
    rm -rf ~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu
    mv ~/.rustup/toolchains/nightly-$pin_date-x86_64-unknown-linux-gnu ~/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu
else
    echo "Unknown operating system"
fi
