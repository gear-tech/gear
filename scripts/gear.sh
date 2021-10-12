#!/usr/bin/env sh

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
SCRIPTS="$ROOT_DIR/scripts/src"
TARGET_DIR="$ROOT_DIR/target"

. $SCRIPTS/build.sh
. $SCRIPTS/check.sh
. $SCRIPTS/clippy.sh
. $SCRIPTS/common.sh
. $SCRIPTS/docker.sh
. $SCRIPTS/format.sh
. $SCRIPTS/init.sh
. $SCRIPTS/test.sh

COMMAND="$1"
shift

case "$COMMAND" in
    -h | --help) gear_usage; exit; ;;
    -s | --show) show; exit; ;;

    build) case "$1" in
                -h | --help) build_usage; exit; ;;

                gear) header "Building gear workspace"
                        shift; gear_build "$@"; ;;
                examples) header "Building gear examples"
                        shift; examples_build "$ROOT_DIR" "$TARGET_DIR" "$@"; ;;
                wasm-proc) header "Building wasm-proc util";
                        shift; wasm_proc_build; ;;
                examples-proc) header "Processing examples via wasm-proc";
                        shift; examples_proc "$TARGET_DIR"; ;;
                node) header "Building gear node";
                        shift; node_build "$@"; ;;

                *) header "Unknown option: $1"; build_usage; exit 1; ;;
        esac;;

    check) case "$1" in
                -h | --help) check_usage; exit; ;;

                gear) header "Checking gear workspace compile"
                        shift; gear_check "$@"; ;;
                examples) header "Checking gear examples compile"
                        shift; examples_check "$ROOT_DIR" "$TARGET_DIR"; ;;
                benchmark) header "Checking node benchmarks compile"
                        shift; benchmark_check; ;;

                *) header "Unknown option: $1"; check_usage; exit 1; ;;
        esac;;

    clippy) case "$1" in
                -h | --help) clippy_usage; exit; ;;

                gear) header "Checking clippy errors of gear workspace"
                        shift; gear_clippy "$@"; ;;
                examples) header "Checking clippy errors of gear program examples"
                        shift; examples_clippy; ;;

                *) header "Unknown option: $1"; clippy_usage; exit 1; ;;
        esac;;

    docker) case "$1" in
                -h | --help) docker_usage; exit; ;;

                run) header "Running docker"
                        shift; echo docker_run; ;;

                *) header "Unknown option: $1"; docker_usage; exit 1; ;;
        esac;;

    format) case "$1" in
                -h | --help) format_usage; exit; ;;

                gear) header "Formatting gear workspace"
                        shift; format "$ROOT_DIR/Cargo.toml" "$@"; ;;
                examples) header "Formatting gear program examples"
                        shift; format "$ROOT_DIR/examples/Cargo.toml" "$@"; ;;
                doc) header "Formatting gear doc"
                        shift; doc_format "$@"; ;;

                *) header "Unknown option: $1"; format_usage; exit 1; ;;
        esac;;

    init) case "$1" in
                -h | --help) init_usage; exit; ;;

                wasm) header "Initializing WASM environment"
                        shift; wasm_init; ;;
                js) header "Syncing JS packages"
                        shift; js_init; ;;

                *) header "Unknown option: $1"; init_usage; exit 1; ;;
        esac;;

    test) case "$1" in
                -h | --help) test_usage; exit; ;;

                gear) header "Running gear tests"
                        shift; workspace_test "$@"; ;;
                js) header "Running js tests"
                        shift; js_test "$ROOT_DIR"; ;;
                gtest) header "Running gtest"
                        shift; gtest "$ROOT_DIR" "$@"; ;;
                ntest) header "Running node testsuite"
                        shift; ntest "$ROOT_DIR"; ;;

                *) header "Unknown option: $1"; test_usage; exit 1; ;;
        esac;;

        *) header "Unknown option: $COMMAND"; gear_usage; exit 1; ;;
esac
