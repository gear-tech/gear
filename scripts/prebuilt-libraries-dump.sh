#!/usr/bin/env bash

if [ "$#" -ne 1 ]; then
    echo "Usage: source $0 <prebuilt-libraries-directory>"
    exit 1
fi

if [ ! -d "$1" ]; then
    echo "Error: '$1' is not a valid directory or does not exist."
    exit 1
fi

PREBUILT_LIBRARIES_DIR=$(realpath $1)

cargo clean
CC_DUMP_LIBRARIES_PATH="$PREBUILT_LIBRARIES_DIR" ./scripts/gear.sh build node --release --locked

HOST_TARGET=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")
find . -name libjemalloc_pic.a | grep "out/lib" | xargs -I {} cp {} "$PREBUILT_LIBRARIES_DIR/$HOST_TARGET"

cargo clean
