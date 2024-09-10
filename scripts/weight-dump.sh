#!/usr/bin/env bash

set -e

dump_path="weight-dumps"
mkdir -p "$dump_path"

current_branch=$(git branch --show-current)

dump_path1="$dump_path/${current_branch//\//-}.json"
cargo run --package gear-weight-diff --release -- dump "$dump_path1" --label "$current_branch"
cargo run --quiet --package gear-weight-diff --release -- codegen "$dump_path1" vara > core/src/gas_metering/schedule_tmp.rs
mv core/src/gas_metering/schedule_tmp.rs core/src/gas_metering/schedule.rs
cargo fmt
