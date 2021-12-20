#!/usr/bin/env sh

build_usage() {
  cat << EOF

  Usage:
    ./gear.sh build <FLAG>
    ./gear.sh build <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           build gear workspace
    examples       build gear program examples
    wasm-proc      build wasm-proc util
    examples-proc  process built examples via wasm-proc
    node           build node

EOF
}

gear_build() {
  cargo build --workspace "$@"
}

node_build() {
  cargo build -p gear-node "$@"
}

wasm_proc_build() {
  cargo build -p wasm-proc --release
}

# $1 = TARGET DIR
examples_proc() {
  "$1"/release/wasm-proc -p "$1"/wasm32-unknown-unknown/release/*.wasm
}

# $1 = ROOT DIR, $2 = TARGET DIR
examples_build() {
  ROOT_DIR="$1"
  TARGET_DIR="$2"
  shift
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
    cd "$ROOT_DIR"/examples
    CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release --workspace "$@"
    cd "$ROOT_DIR"
  else
    for yaml in $YAMLS
    do
      names=$(cat $yaml | perl -ne 'print "$1 " if /.*path: .*\/(.*).wasm/s')
      names=$(echo $names | tr _ -)
      for name in $names
      do
        path=$(grep -rbnl --include \*.toml \"$name\" "$ROOT_DIR"/examples/)
        path=$(echo $path | perl -ne 'print $1 if /(.*)Cargo\.toml/s')
        cd $path
        CARGO_TARGET_DIR="$TARGET_DIR" cargo +nightly hack build --release "$@"
        cd -
      done
    done
  fi
}
