#!/usr/bin/env sh

clippy_usage() {
  cat << EOF

  Usage: ./gear.sh clippy [subcommand] [RUST_FLAGS]

  Subcommands:
    -h, --help     show help message and exit

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

examples_clippy() {
  cargo +nightly clippy --workspace --release --no-deps -- \
    -A clippy::missing_safety_doc \
	-A clippy::stable_sort_primitive \
    -D warnings
}
