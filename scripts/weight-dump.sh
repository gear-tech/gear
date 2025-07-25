#!/usr/bin/env bash

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CORE_DIR="$SCRIPT_DIR/../core"

dump_path="weight-dumps"
mkdir -p "$SCRIPT_DIR/../$dump_path"

current_branch=$(git branch --show-current)

dump_path1="$dump_path/${current_branch//\//-}.json"
cargo run --package gear-weight-diff --release -- dump "$dump_path1" --label "$current_branch"
cargo run --quiet --package gear-weight-diff --release -- codegen "$dump_path1" vara > "$CORE_DIR"/src/gas_metering/schedule_tmp.rs
mv "$CORE_DIR"/src/gas_metering/schedule_tmp.rs "$CORE_DIR"/src/gas_metering/schedule.rs
cargo fmt
