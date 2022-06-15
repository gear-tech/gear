#!/usr/bin/env bash

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"

MAIN_BRANCH='master'

CURRENT_BRANCH=$1
if [ -z "$CURRENT_BRANCH" ]; then
    CURRENT_BRANCH=`git branch --show-current`
fi

COUNT=$2
if [ -z "$COUNT" ]; then
    COUNT=100
fi

git checkout $MAIN_BRANCH

mkdir $ROOT_DIR/target/tests/
for i in `seq 1 $COUNT`; do
    $ROOT_DIR/scripts/gear.sh test gear --release
    mv $ROOT_DIR/target/nextest/ci/junit.xml $ROOT_DIR/target/tests/$i
done

mkdir $ROOT_DIR/target/runtime-tests/
for i in `seq 1 $COUNT`; do
    $ROOT_DIR/scripts/gear.sh test rtest
    mv $ROOT_DIR/target/runtime-test-junit.xml $ROOT_DIR/target/runtime-tests/$i
done

mkdir $ROOT_DIR/target/main_branch/
cargo run --package regression-analysis --release -- collect-data --data-folder-path $ROOT_DIR/target/tests/ --output-path $ROOT_DIR/target/main_branch/pallet-tests.json
cargo run --package regression-analysis --release -- collect-data --disable-filter --data-folder-path $ROOT_DIR/target/runtime-tests/ --output-path $ROOT_DIR/target/main_branch/runtime-tests.json

git checkout $CURRENT_BRANCH

$ROOT_DIR/scripts/gear.sh test gear --release
$ROOT_DIR/scripts/gear.sh test rtest

cargo run --package regression-analysis --release -- compare --data-path $ROOT_DIR/target/main_branch/pallet-tests.json --current-junit-path $ROOT_DIR/target/nextest/ci/junit.xml
cargo run --package regression-analysis --release -- compare --disable-filter --data-path $ROOT_DIR/target/main_branch/runtime-tests.json --current-junit-path $ROOT_DIR/target/runtime-test-junit.xml
