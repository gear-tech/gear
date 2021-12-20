#!/usr/bin/env sh

test_usage() {
  cat << EOF

  Usage:
    ./gear.sh test <FLAG>
    ./gear.sh test <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           run workspace tests
    js             run metadata js tests
    gtest          run gtest testing tool
    ntest          run node testsuite
    pallet         run pallet-gear tests

EOF
}

workspace_test() {
  cargo test --workspace "$@"
}

# $1 - ROOT DIR
js_test() {
  node "$1"/utils/wasm-proc/metadata-js/test.js
}

gtest() {
  ROOT_DIR="$1"
  shift

  if [ -n "$1" ]
  then
    has_yamls=$(echo "${1}" | grep "yamls=")
  fi

  if  [ -n "$has_yamls" ]
  then
    if ! command -v perl &> /dev/null
    then
      echo "could parse yamls only with \"perl\" installed"
      exit 1
    fi

    YAMLS=$(echo $1 | perl -ne 'print $1 if /yamls=(.*)/s')
    shift
  fi

  if [ -z "$YAMLS" ]
  then
    YAMLS="$ROOT_DIR"/gtest/spec/*.yaml
  fi

  cargo run --package gear-test --release -- $YAMLS "$@"
}

# $1 - ROOT DIR
ntest() {
  cargo run --package gear-node --release -- runtests "$1"/gtest/spec/*.yaml
}

pallet_test() {
  cargo test -p pallet-gear "$@"
}
