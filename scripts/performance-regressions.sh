#!/usr/bin/env sh

MAIN_BRANCH='master'

CURRENT_BRANCH=$1
if [ x$CURRENT_BRANCH = x ]; then
    CURRENT_BRANCH=`git branch --show-current`
fi

COUNT=$2
if [ x$COUNT = x ]; then
    COUNT=100
fi

git checkout $MAIN_BRANCH

mkdir ./target/tests/
for i in `seq 1 $COUNT`; do
    ./scripts/gear.sh test gear --release
    mv ./target/nextest/ci/junit.xml ./target/tests/$i
done

mkdir ./target/runtime-tests/
for i in `seq 1 $COUNT`; do
    ./scripts/gear.sh test rtest
    mv ./target/runtime-test-junit.xml ./target/runtime-tests/$i
done

mkdir ./target/main_branch/
cargo run --package regression-analysis --release -- collect-data --data-folder-path ./target/tests/ --output-path ./target/main_branch/pallet-tests.json
cargo run --package regression-analysis --release -- collect-data --disable-filter --data-folder-path ./target/runtime-tests/ --output-path ./target/main_branch/runtime-tests.json

git checkout $CURRENT_BRANCH

./scripts/gear.sh test gear --release
./scripts/gear.sh test rtest

cargo run --package regression-analysis --release -- compare --data-path ./target/main_branch/pallet-tests.json --current-junit-path ./target/nextest/ci/junit.xml
cargo run --package regression-analysis --release -- compare --disable-filter --data-path ./target/main_branch/runtime-tests.json --current-junit-path ./target/runtime-test-junit.xml
