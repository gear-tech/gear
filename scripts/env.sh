#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"

bold() {
    tput bold
}

normal() {
    tput sgr0
}

wasm_init() {
    if [ -z $CI_PROJECT_NAME ] ; then
        rustup update nightly
        rustup update stable
    fi

    rustup target add wasm32-unknown-unknown --toolchain nightly
}

js_init() {
    npm --prefix $ROOT_DIR/utils/wasm-proc/metadata-js install
    npm --prefix $ROOT_DIR/utils/wasm-proc/metadata-js update
    npm --prefix $ROOT_DIR/gtest/src/js install
    npm --prefix $ROOT_DIR/gtest/src/js update
}

show() {
    rustup show

    bold && echo "node.js\n-------\n" && normal
	node -v

    bold && echo "\nnpm\n---\n" && normal
	npm -v
}

docker_run() {
    bold && echo "*** Start Substrate node template ***\n" && normal
    docker-compose down --remove-orphans
    docker-compose run --rm --service-ports dev $@
}

case "$1" in
    init)
            bold && echo "*** Initializing WASM build environment\n" && normal
            wasm_init
            bold && echo "\n*** Installing and updating JS dependencies\n" && normal
            js_init
            ;;
    wasm)
            bold && echo "*** Initializing WASM build environment\n" && normal
            wasm_init
            ;;
    js)
            bold && echo "*** Installing and updating JS dependencies\n" && normal
            js_init
            ;;
    show)
            bold && echo "Your environment:\n" && normal
            show
            ;;
    docker)
            docker_run
            ;;
esac
