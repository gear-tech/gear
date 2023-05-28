#!/usr/bin/env bash

# This is a helper script for generating comparison tables between branches

set -ex

help() {
  cat <<'EOF'
Generate comparison tables with weights between branches

USAGE:
    ./weight-diff.sh <BRANCH1> <BRANCH2> <RUNTIME> <KIND>

ARGUMENTS:
  <BRANCH1>  branch #1 from where to get the weights
  <BRANCH2>  branch #2 from where to get the weights
  <RUNTIME>  what runtime to compare? [possible values: gear, vara]
  <KIND>     for which weights to generate a table? [possible values: instruction, host-fn]
EOF
}

if [ $# -ne 4 ]; then
  help
  exit 1
fi

current_branch=$(git branch --show-current)

branch1=$1
branch2=$2
runtime=$3
kind=$4

dump_path="weight-dumps"
mkdir -p "$dump_path"

git checkout "$branch1"
dump_path1="$dump_path/$branch1.json"
cargo run --package gear-weight-diff --release -- dump "$dump_path1" --label "$branch1"

git checkout "$branch2"
dump_path2="$dump_path/$branch2.json"
cargo run --package gear-weight-diff --release -- dump "$dump_path2" --label "$branch2"

git checkout "$current_branch"

cargo run --package gear-weight-diff --release -- diff "$dump_path1" "$dump_path2" "$runtime" "$kind"
