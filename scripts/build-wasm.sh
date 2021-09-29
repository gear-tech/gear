#!/usr/bin/env sh

set -e

ROOT_DIR="$(cd "$(dirname "$0")"/../examples && pwd)"

# Get newline-separated list of all workspace members in `$1/Cargo.toml`
get_members() {
  tr -d "\n" < "$1/Cargo.toml" |
    sed -n -e 's/.*members[[:space:]]*=[[:space:]]*\[\([^]]*\)\].*/\1/p' |
    sed -n -e 's/,/ /gp' |
    sed -n -e 's/"\([^"]*\)"/\1/gp'
}

# For each entry in Cargo.toml workspace members:
for entry in $(get_members $ROOT_DIR); do
  # Quotes around `$entry` are not used intentionally to support globs in entry syntax, e.g. "member/*"
  for member in "$ROOT_DIR"/$entry; do
    cd "$member"
    cargo +nightly build --release
  done
done

rm -f $ROOT_DIR/target/wasm32-unknown-unknown/release/*.opt.wasm
rm -f $ROOT_DIR/target/wasm32-unknown-unknown/release/*.meta.wasm

cd $ROOT_DIR/..
cargo build -p wasm-proc --release

$ROOT_DIR/../target/release/wasm-proc -p $ROOT_DIR/target/wasm32-unknown-unknown/release/*.wasm
