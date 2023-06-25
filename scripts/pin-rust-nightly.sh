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
suffix=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")
rustup toolchain install nightly-$pin_date --component llvm-tools-preview
rustup target add wasm32-unknown-unknown --toolchain nightly-$pin_date
rm -rf ~/.rustup/toolchains/nightly-$suffix
ln -s ~/.rustup/toolchains/nightly-$pin_date-$suffix ~/.rustup/toolchains/nightly-$suffix
