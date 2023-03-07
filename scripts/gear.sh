#!/usr/bin/env sh

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"
SCRIPTS="$ROOT_DIR/scripts/src"
TARGET_DIR="$ROOT_DIR/target"
CARGO_HACK="hack"
CARGO_NEXTEST="nextest"
CARGO="cargo"
EXE_RUNNER=""
EXE_EXTENSION=""

. "$SCRIPTS"/common.sh

if [ "$CARGO_BUILD_TARGET" = "x86_64-pc-windows-msvc" ]; then
  TARGET_DIR="$TARGET_DIR/x86_64-pc-windows-msvc"
  CARGO="cargo xwin"
  EXE_RUNNER="wine"
  EXE_EXTENSION=".exe"
  header "Using cargo-xwin"
fi

. "$SCRIPTS"/build.sh
. "$SCRIPTS"/check.sh
. "$SCRIPTS"/clippy.sh
. "$SCRIPTS"/coverage.sh
. "$SCRIPTS"/docker.sh
. "$SCRIPTS"/format.sh
. "$SCRIPTS"/init.sh
. "$SCRIPTS"/run.sh
. "$SCRIPTS"/test.sh

show() {
  rustup show

  bold && printf "node.js\n-------\n\n" && normal
  node -v

  bold && printf "\nnpm\n---\n\n" && normal
  npm -v
}

check_extensions() {
  if [ -z "$(cargo --list | awk '{print $1}' | grep "^$CARGO_HACK$")" ] || [ -z "$(cargo --list | awk '{print $1}' | grep "^$CARGO_NEXTEST$")" ]
    then
      "$SELF" init cargo
  fi
}

gear_usage() {
  cat << EOF

  Usage:
    ./gear.sh <FLAG>
    ./gear.sh <COMMAND> <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Commands:
    help           show help message and exit
    show           show env versioning and installed toolchains

    build          build gear parts
    check          check that gear parts are compilable
    clippy         check clippy errors for gear parts
    docker         docker functionality
    format         format gear parts via rustfmt
    init           initializes and updates packages and toolchains
    run            run gear node
    test           test tool
    coverage       coverage utilities

  Try ./gear.sh <COMMAND> -h (or --help) to learn more about each command.

  The ./gear.sh requires the 'сargo-hack' extension sometime.
  If it's not found, it will be installed automatically.

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

  show)
    header "Showing installed tools"
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

      gear-test)
        header "Building gear test"
        gear_test_build "$@"; ;;

      examples)
        check_extensions
        header "Building gear examples"
        examples_build "$ROOT_DIR" "$TARGET_DIR" "$@"; ;;

      wasm-proc)
        header "Building wasm-proc util"
        wasm_proc_build; ;;

      examples-proc)
        header "Processing examples via wasm-proc"
        examples_proc "$TARGET_DIR"; ;;

      node)
        header "Building gear node"
        node_build "$@"; ;;

      wat-examples)
        header "Build wat examples"
        wat_examples_build "$ROOT_DIR" "$TARGET_DIR" "$@"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        build_usage
        exit 1; ;;
    esac;;

  check)
    case "$SUBCOMMAND" in
      -h | --help | help)
        check_usage
        exit; ;;

      gear)
        header "Checking gear workspace"
        gear_check "$@"; ;;

      examples)
        check_extensions
        header "Checking gear examples"
        examples_check "$ROOT_DIR" "$TARGET_DIR"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        check_usage
        exit 1; ;;
    esac;;

  clippy)
    case "$SUBCOMMAND" in
      -h | --help | help)
        clippy_usage
        exit; ;;

      gear)
        header "Invoking clippy on gear workspace"
        gear_clippy "$@"; ;;

      examples)
        check_extensions
        header "Invoking clippy on gear examples"
        examples_clippy "$ROOT_DIR"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
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
        header  "Unknown option: '$SUBCOMMAND'"
        docker_usage
        exit 1; ;;
    esac;;

  format)
    CHECK=false
    for flag in "$@"
      do [ "$flag" = "--check" ] && CHECK="true"
    done

    case "$SUBCOMMAND" in
      -h | --help | help)
        format_usage
        exit; ;;

      gear)
        if [ "$CHECK" = "true" ]
          then header "Checking gear workspace formatting"
          else header "Formatting gear workspace"
        fi
        format "$ROOT_DIR/Cargo.toml" "$@"; ;;

      examples)
        if [ "$CHECK" = "true" ]
          then header "Checking gear examples formatting"
          else header "Formatting gear examples"
        fi
        format "$ROOT_DIR/examples/Cargo.toml" "$@"; ;;

      doc)
        if [ "$CHECK" = "true" ]
          then header "Checking gear doc formatting"
          else header "Formatting gear doc"
        fi
        doc_format "$@"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
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

      update-js)
        header "Updating JS packages"
        js_update "$ROOT_DIR"; ;;

      cargo)
        header "Installing cargo extensions '$CARGO_HACK' and(/or) '$CARGO_NEXTEST'"
        cargo_init; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        init_usage
        exit 1; ;;
    esac;;

  run)
    case "$SUBCOMMAND" in
      -h | --help | help)
        run_usage
        exit; ;;

      node)
        header "Running gear node"
        run_node "$@"; ;;

      purge-chain)
        header "Purging gear node chain"
        purge_chain "$@"; ;;

      purge-dev-chain)
        header "Purging gear dev node chain"
        purge_dev_chain "$@"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        run_usage
        exit 1; ;;
    esac;;

  test)
    case "$SUBCOMMAND" in
      -h | --help | help)
        test_usage
        exit; ;;

      gear)
        check_extensions
        header "Running gear tests"
        workspace_test "$@"; ;;

      gcli)
        header "Running gcli tests"
        gcli_test "$@"; ;;

      js)
        header "Running js tests"
        js_test "$ROOT_DIR"; ;;

      gtest)
        header "Running gear-test (spec testing)"
        gtest "$ROOT_DIR" "$@"; ;;

      validators)
        header "Checking validators"
        validators "$ROOT_DIR" "$@"; ;;

      rtest)
        header "Running node runtime testsuite"
        rtest "$ROOT_DIR" "$TARGET_DIR" "$@"; ;;

      pallet)
        header "Running pallet-gear tests"
        pallet_test "$@"; ;;

      client)
        header "Running gclient tests"
        client_tests "$@"; ;;

      fuzz)
        header "Running fuzzer for runtime panic checks"
        run_fuzzer "$ROOT_DIR" ;;

      syscalls)
        header "Running syscalls integrity test of pallet-gear 'benchmarking' module on WASMI executor"
        syscalls_integrity_test "$@"; ;;

      doc)
        header "Testing examples in docs"
        doc_test "$ROOT_DIR/Cargo.toml" "$@";
        doc_test "$ROOT_DIR/examples/Cargo.toml" "$@"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        test_usage
        exit 1; ;;
    esac;;

  coverage)
    case "$SUBCOMMAND" in
      -h | --help | help)
        coverage_usage
        exit; ;;

      gtest)
        header "Running gear-test (spec testing)"
        gtest_debug "$ROOT_DIR" "$@"; ;;

      rtest)
        header "Running node runtime testsuite"
        rtest_debug "$ROOT_DIR"; ;;

      *)
        header  "Unknown option: '$SUBCOMMAND'"
        coverage_usage
        exit 1; ;;
    esac;;

  *)
    header "Unknown option: '$COMMAND'"
    gear_usage
    exit 1; ;;
esac
