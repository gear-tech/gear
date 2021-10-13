#!/usr/bin/env sh

set -e

ROOT_DIR="$(cd "$(dirname "$0")"/.. && pwd)"
SCRIPTS="$ROOT_DIR/scripts/src"
TARGET_DIR="$ROOT_DIR/target"

. "$SCRIPTS"/build.sh
. "$SCRIPTS"/check.sh
. "$SCRIPTS"/clippy.sh
. "$SCRIPTS"/common.sh
. "$SCRIPTS"/docker.sh
. "$SCRIPTS"/format.sh
. "$SCRIPTS"/init.sh
. "$SCRIPTS"/test.sh

show() {
  rustup show

  header "node.js\n-------\n"
  node -v

  header "\nnpm\n---\n"
  npm -v
}

gear_usage() {
  cat << EOF

  Usage: ./gear.sh [command] [subcommand] [OPTIONAL]

  Commands:
    -h, --help     show help message and exit
    -s, --show     show env versioning and installed toolchains

    build          build gear parts
    check          check that gear parts are compilable
    clippy         check clippy errors for gear parts
    docker         docker functionality
    format         format gear parts via rustfmt
    init           initializes and updates packages and toolchains
    test           test tool
    
  Try ./gear.sh -h (or --help) to learn more about each command.

EOF
}

COMMAND="$1"
if [ "$#" -ne  "0" ]
then
  shift
fi

SUBCOMMAND="$1"
if [ "$#" -ne  "0" ]
then
    shift
fi

case "$COMMAND" in
  -h | --help | help)
    gear_usage
    exit; ;;

  -s | --show | show)
    show
    exit; ;;

  build)
    case "$SUBCOMMAND" in
      -h | --help | help)
        build_usage
        exit; ;;

      gear)
        header "Building gear workspace"
        gear_build "$@"; ;;

      examples)
        header "Building gear examples"
        examples_build "$ROOT_DIR" "$TARGET_DIR"; ;;

      wasm-proc)
        header "Building wasm-proc util"
        wasm_proc_build; ;;

      examples-proc)
        header "Processing examples via wasm-proc"
        examples_proc "$TARGET_DIR"; ;;

      node)
        header "Building gear node"
        node_build "$@"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        build_usage
        exit 1; ;;
    esac;;

  check)
    case "$SUBCOMMAND" in
      -h | --help | help)
        check_usage
        exit; ;;
      
      gear)
        header "Checking gear workspace compile"
        gear_check "$@"; ;;
      
      examples)
        header "Checking gear examples compile"
        examples_check "$ROOT_DIR" "$TARGET_DIR"; ;;

      benchmark)
        header "Checking node benchmarks compile"
        benchmark_check "$@"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        check_usage
        exit 1; ;;
    esac;;

  clippy)
    case "$SUBCOMMAND" in
      -h | --help | help)
        clippy_usage
        exit; ;;

      gear)
        header "Checking clippy errors of gear workspace"
        gear_clippy "$@"; ;;
      
      examples)
        header "Checking clippy errors of gear program examples"
        examples_clippy; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        clippy_usage
        exit 1; ;;
    esac;;

  docker)
    case "$SUBCOMMAND" in
      -h | --help | help)
        docker_usage
        exit; ;;

      run)
        header "Running docker"
        docker_run "$@"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        docker_usage
        exit 1; ;;
    esac;;

  format)
    case "$SUBCOMMAND" in
      -h | --help | help)
        format_usage
        exit; ;;

      gear)
        header "Formatting gear workspace"
        format "$ROOT_DIR/Cargo.toml" "$@"; ;;

      examples)
        header "Formatting gear program examples"
        format "$ROOT_DIR/examples/Cargo.toml" "$@"; ;;

      doc)
        header "Formatting gear doc"
        doc_format "$@"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        format_usage
        exit 1; ;;
    esac;;

  init)
    case "$SUBCOMMAND" in
      -h | --help | help)
        init_usage
        exit; ;;

      wasm)
        header "Initializing WASM environment"
        wasm_init; ;;

      js)
        header "Syncing JS packages"
        js_init "$ROOT_DIR"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        init_usage
        exit 1; ;;
    esac;;

  test)
    case "$SUBCOMMAND" in
      -h | --help | help)
        test_usage
        exit; ;;

      gear)
        header "Running gear tests"
        workspace_test "$@"; ;;

      js)
        header "Running js tests"
        js_test "$ROOT_DIR"; ;;

      gtest)
        header "Running gtest"
        gtest "$ROOT_DIR" "$@"; ;;

      ntest)
        header "Running node testsuite"
        ntest "$ROOT_DIR"; ;;

      *)
        header "Unknown option: $SUBCOMMAND"
        test_usage
        exit 1; ;;
    esac;;

  *)
    header "Unknown option: $COMMAND"
    gear_usage
    exit 1; ;;
esac
