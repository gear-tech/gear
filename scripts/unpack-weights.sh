#!/usr/bin/env bash

# This is a helper script for updating weights from artifacts

set -e

say() {
  echo "$@"
}

say_err() {
  say "$@" >&2
}

err() {
  if [ ! -z ${td-} ]; then
    rm -rf $td
  fi

  say_err "error: $@"
  exit 1
}

need() {
  if ! command -v $1 >/dev/null 2>&1; then
    err "need $1 (command not found)"
  fi
}

# Dependencies
need unzip

GEAR_RUNTIME="weights-gear.zip"
VARA_RUNTIME="weights-vara.zip"

if [[ ! -f $GEAR_RUNTIME ]] || [[ ! -f $VARA_RUNTIME ]]; then
  echo "You need to download artifacts with weights before unpacking: $GEAR_RUNTIME, $VARA_RUNTIME"
  echo "Please follow the link: https://github.com/gear-tech/gear/actions/workflows/benchmarks.yml"
  exit 1
fi

# extract artifacts to the correct directories
unzip -o $GEAR_RUNTIME -d runtime/gear/src/weights/ && rm $GEAR_RUNTIME
unzip -o $VARA_RUNTIME -d runtime/vara/src/weights/ && rm $VARA_RUNTIME

# apply some patches for `pallets/gear/src/weights.rs`
cp runtime/gear/src/weights/pallet_gear.rs pallets/gear/src/weights.rs
sed -i -E 's/\w+::WeightInfo for SubstrateWeight/WeightInfo for SubstrateWeight/' pallets/gear/src/weights.rs
