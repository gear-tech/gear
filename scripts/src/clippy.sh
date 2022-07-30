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
  SKIP_WASM_BUILD=1 cargo +nightly clippy --workspace "$@" -- --no-deps \
                    -A clippy::assertions_on_result_states `# FIXME: consider removing this line after #1257.` \
                    -D warnings
}

# $1 - ROOT DIR
examples_clippy() {
  cd "$1"/examples
  SKIP_WASM_BUILD=1 cargo +nightly hack clippy --workspace --release -- --no-deps \
	  -A clippy::stable_sort_primitive \
    -A clippy::assertions_on_result_states `# FIXME: consider removing this line after #1257.` \
    -D warnings
  cd "$1"
}
