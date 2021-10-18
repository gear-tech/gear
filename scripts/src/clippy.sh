#!/usr/bin/env sh

clippy_usage() {
  cat << EOF

  Usage:
    ./gear.sh clippy <FLAG>
    ./gear.sh clippy <SUBCOMMAND> [CARGO FLAGS]

  Flags:
    -h, --help     show help message and exit

  Subcommands:
    help           show help message and exit

    gear           check gear workspace for clippy errors
    examples       check gear program examples for clippy errors

EOF
}

gear_clippy() {
  cargo +nightly clippy --workspace "$@" \
    --all-features \
    --no-deps \
    -- -D warnings
}

# $1 - ROOT DIR
examples_clippy() {
  cd "$1"/examples
  cargo +nightly clippy --workspace --release --no-deps -- \
    -A clippy::missing_safety_doc \
	  -A clippy::stable_sort_primitive \
    -D warnings
  cd "$1"
}
