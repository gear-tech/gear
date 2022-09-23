#!/bin/bash
#
# gear-node downloader

######################
# Usage of this script.
########################
function usage() {
    cat 1>&2 <<EOF
download-gear
Download gear-node

USAGE:
    download-gear.sh <DIRECTORY>
EOF
}

###################
# Download gear-node.
#######################
function download-gear() {
    url='https://builds.gear.rs/gear-nightly-linux-x86_64.tar.xz'

    # Doesn't support Win for now.
    if [[ "$(uname)" == 'Darwin' ]]; then
        if [[ "$(uname -m)" == 'arm64' ]]; then
            url='https://builds.gear.rs/gear-nightly-macos-m1.tar.gz'
        else
            url='https://builds.gear.rs/gear-nightly-macos-x86_64.tar.gz'
        fi
    fi

    if [ -n "$1" ]; then
        curl "${url}" | tar xfJ - -C "$1"
    else
        usage
    fi
}


###################
# Download gear-node.
######################
function main() {
    download-gear "$@"
}

main "$@"
