#!/usr/bin/env bash

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"

MAIN_BRANCH='master'

COUNT=$1
if [ -z "$COUNT" ]; then
    COUNT=100
fi

CURRENT_BRANCH=$2
if [ -z "$CURRENT_BRANCH" ]; then
    CURRENT_BRANCH=`git branch --show-current`
fi

echo 'CURRENT_BRANCH = '$CURRENT_BRANCH' , COUNT = '$COUNT', ROOT_DIR = '$ROOT_DIR

WARMUP_COUNT=3

git checkout $MAIN_BRANCH

make gear-release
make examples

mkdir -p $ROOT_DIR/target/tests/
mkdir -p $ROOT_DIR/target/tests-output/
for i in `seq 1 $WARMUP_COUNT`; do
    $ROOT_DIR/scripts/gear.sh test gear --release > $ROOT_DIR/target/tests-output/$i 2>&1
done

for i in `seq 1 $COUNT`; do
    echo $i
    $ROOT_DIR/scripts/gear.sh test gear --release > $ROOT_DIR/target/tests-output/$i 2>&1
    mv $ROOT_DIR/target/nextest/ci/junit.xml $ROOT_DIR/target/tests/$i
done

mkdir -p $ROOT_DIR/target/runtime-tests/
mkdir -p $ROOT_DIR/target/runtime-tests-output/
for i in `seq 1 $WARMUP_COUNT`; do
    $ROOT_DIR/scripts/gear.sh test rtest > $ROOT_DIR/target/runtime-tests-output/$i 2>&1
done

for i in `seq 1 $COUNT`; do
    echo $i
    $ROOT_DIR/scripts/gear.sh test rtest > $ROOT_DIR/target/runtime-tests-output/$i 2>&1
    mv $ROOT_DIR/target/runtime-test-junit.xml $ROOT_DIR/target/runtime-tests/$i
done

rm -rf $ROOT_DIR/target/main_branch/
mkdir -p $ROOT_DIR/target/main_branch/
cargo run --package regression-analysis --release -- collect-data --data-folder-path $ROOT_DIR/target/tests/ --output-path $ROOT_DIR/target/main_branch/pallet-tests.json
cargo run --package regression-analysis --release -- collect-data --disable-filter --data-folder-path $ROOT_DIR/target/runtime-tests/ --output-path $ROOT_DIR/target/main_branch/runtime-tests.json

git checkout $CURRENT_BRANCH

make gear-release
make examples

rm -rf $ROOT_DIR/target/tests/*
for i in `seq 1 $WARMUP_COUNT`; do
    $ROOT_DIR/scripts/gear.sh test gear --release > $ROOT_DIR/target/tests-output/$i 2>&1
done

for i in `seq 1 $COUNT`; do
    echo $i
    $ROOT_DIR/scripts/gear.sh test gear --release > $ROOT_DIR/target/tests-output/$i 2>&1
    mv $ROOT_DIR/target/nextest/ci/junit.xml $ROOT_DIR/target/tests/$i
done

rm -rf $ROOT_DIR/target/runtime-tests/*
for i in `seq 1 $WARMUP_COUNT`; do
    $ROOT_DIR/scripts/gear.sh test rtest > $ROOT_DIR/target/runtime-tests-output/$i 2>&1
done

for i in `seq 1 $COUNT`; do
    echo $i
    $ROOT_DIR/scripts/gear.sh test rtest > $ROOT_DIR/target/runtime-tests-output/$i 2>&1
    mv $ROOT_DIR/target/runtime-test-junit.xml $ROOT_DIR/target/runtime-tests/$i
done

rm -rf $ROOT_DIR/target/current_branch/
mkdir -p $ROOT_DIR/target/current_branch/
cargo run --package regression-analysis --release -- collect-data --data-folder-path $ROOT_DIR/target/tests/ --output-path $ROOT_DIR/target/current_branch/pallet-tests.json
cargo run --package regression-analysis --release -- collect-data --disable-filter --data-folder-path $ROOT_DIR/target/runtime-tests/ --output-path $ROOT_DIR/target/current_branch/runtime-tests.json

#
# It is highly recommended to use python3 virtualenv for managing required dependencies.
# Activate virtual environment before running this script: $ source /path/to/venv/bin/activate.
# Prerequisites: $ pip3 install seaborn pandas
#

python3 $ROOT_DIR/scripts/performance-boxplot.py $ROOT_DIR/target/main_branch/pallet-tests.json $ROOT_DIR/target/current_branch/pallet-tests.json
mv ./results $ROOT_DIR/target/performance-pallet-tests

python3 $ROOT_DIR/scripts/performance-boxplot.py $ROOT_DIR/target/main_branch/runtime-tests.json $ROOT_DIR/target/current_branch/runtime-tests.json
mv ./results $ROOT_DIR/target/performance-runtime-tests
