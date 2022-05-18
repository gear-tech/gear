#!/usr/bin/env sh

MAIN_BRANCH='master'

CURRENT_BRANCH=$1
if [ x$CURRENT_BRANCH = x ]; then
    CURRENT_BRANCH=`git branch --show-current`
fi

git checkout $MAIN_BRANCH

./scripts/gear.sh test gear --release
./scripts/gear.sh test rtest

mkdir ./target/main_branch/
mv ./target/runtime-test-junit.xml ./target/main_branch/
mv ./target/nextest/ci/junit.xml ./target/main_branch/pallet-test-junit.xml

git checkout $CURRENT_BRANCH

./scripts/gear.sh test gear --release
./scripts/gear.sh test rtest

cargo run --package regression-analysis --release -- --master-junit-xml ./target/main_branch/pallet-test-junit.xml --current-junit-xml ./target/nextest/ci/junit.xml
cargo run --package regression-analysis --release -- --disable-filter --master-junit-xml ./target/main_branch/runtime-test-junit.xml --current-junit-xml ./target/runtime-test-junit.xml
