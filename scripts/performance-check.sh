#!/usr/bin/env bash

#
# It is highly recommended to use python3 virtualenv for managing required dependencies.
# Activate virtual environment before running this script: $ source /path/to/venv/bin/activate.
# Prerequisites: (venv) $ pip3 install seaborn pandas
#
# ./scripts/performance-check.sh [COUNT [BOXPLOT [BRANCH]]]
#

set -e

SELF="$0"
ROOT_DIR="$(cd "$(dirname "$SELF")"/.. && pwd)"

MAIN_BRANCH='master'

COUNT=$1
if [ -z "$COUNT" ]; then
    COUNT=100
fi

BOXPLOT=$2
if [ -z "$BOXPLOT" ] || [ "$BOXPLOT" == "0" ]; then
    BOXPLOT=0
fi

CURRENT_BRANCH=$3
if [ -z "$CURRENT_BRANCH" ]; then
    CURRENT_BRANCH=`git branch --show-current`
fi

echo 'CURRENT_BRANCH = '$CURRENT_BRANCH' , COUNT = '$COUNT', ROOT_DIR = '$ROOT_DIR', BOXPLOT = '$BOXPLOT

WARMUP_COUNT=3

collect_data() {
    git checkout $1

    make gear-release
    make examples

    rm -rf "$ROOT_DIR/target/tests/"
    mkdir -p "$ROOT_DIR/target/tests/"
    mkdir -p "$ROOT_DIR/target/tests-output/"
    for i in `seq 1 $WARMUP_COUNT`; do
        "$ROOT_DIR/scripts/gear.sh" test gear --release > "$ROOT_DIR/target/tests-output/$i" 2>&1
    done

    for i in `seq 1 $COUNT`; do
        echo $i
        "$ROOT_DIR/scripts/gear.sh" test gear --release > "$ROOT_DIR/target/tests-output/$i" 2>&1
        mv "$ROOT_DIR/target/nextest/ci/junit.xml" "$ROOT_DIR/target/tests/$i"
    done

    rm -rf "$ROOT_DIR/target/runtime-tests/"
    mkdir -p "$ROOT_DIR/target/runtime-tests/"
    mkdir -p "$ROOT_DIR/target/runtime-tests-output/"
    for i in `seq 1 $WARMUP_COUNT`; do
        "$ROOT_DIR/scripts/gear.sh" test rtest > "$ROOT_DIR/target/runtime-tests-output/$i" 2>&1
    done

    for i in `seq 1 $COUNT`; do
        echo $i
        "$ROOT_DIR/scripts/gear.sh" test rtest > "$ROOT_DIR/target/runtime-tests-output/$i" 2>&1
        mv "$ROOT_DIR/target/runtime-test-junit.xml" "$ROOT_DIR/target/runtime-tests/$i"
    done

    rm -rf "$2"
    mkdir -p "$2"
    cargo run --package regression-analysis --release -- collect-data --data-folder-path "$ROOT_DIR/target/tests/" --output-path "$2/pallet-tests.json"
    cargo run --package regression-analysis --release -- collect-data --disable-filter --data-folder-path "$ROOT_DIR/target/runtime-tests/" --output-path "$2/runtime-tests.json"
}


collect_data $MAIN_BRANCH "$ROOT_DIR/target/main_branch"

collect_data $CURRENT_BRANCH "$ROOT_DIR/target/current_branch"

if [ "$BOXPLOT" != "0" ]; then
    python3 "$ROOT_DIR/scripts/performance-boxplot.py" "$ROOT_DIR/target/main_branch/pallet-tests.json" "$ROOT_DIR/target/current_branch/pallet-tests.json"
    rm -rf "$ROOT_DIR/target/performance-pallet-tests"
    mv ./results "$ROOT_DIR/target/performance-pallet-tests"

    python3 "$ROOT_DIR/scripts/performance-boxplot.py" "$ROOT_DIR/target/main_branch/runtime-tests.json" "$ROOT_DIR/target/current_branch/runtime-tests.json"
    rm -rf "$ROOT_DIR/target/performance-runtime-tests"
    mv ./results "$ROOT_DIR/target/performance-runtime-tests"
fi
