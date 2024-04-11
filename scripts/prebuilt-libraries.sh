#!/usr/bin/env bash

unset PREBUILT_LIBRARIES_DIR JEMALLOC_OVERRIDE CC_LIBRARIES_PATH

if [ "$#" -ne 1 ]; then
    echo "Usage: source $0 <prebuilt-libraries-directory>"
    return 1
fi

if [ ! -d "$1" ]; then
    echo "Error: '$1' is not a valid directory or does not exist."
    return 1
fi

PREBUILT_LIBRARIES_DIR=$1
HOST_TARGET=$(rustc -Vv | grep "host: " | sed "s/^host: \(.*\)$/\1/")

export JEMALLOC_OVERRIDE="$PREBUILT_LIBRARIES_DIR/$HOST_TARGET/libjemalloc_pic.a"
export CC_LIBRARIES_PATH="$PREBUILT_LIBRARIES_DIR"

unset HOST_TARGET
