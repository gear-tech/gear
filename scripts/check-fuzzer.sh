#!/usr/bin/env sh

set -e

SELF="$0"
SCRIPTS="$(cd "$(dirname "$SELF")"/ && pwd)"

INITIAL_INPUT_SIZE=${INITIAL_INPUT_SIZE:-'16000000'}

main() {
    echo " >> Getting random bytes from /dev/urandom"
    # Fuzzer expects a minimal input size of 25 MiB. Without providing a corpus of the same or larger
    # size fuzzer will stuck for a long time with trying to test the target using 0..100 bytes.
    mkdir -p utils/runtime-fuzzer/fuzz/corpus/main
    dd if=/dev/urandom of=./check-fuzzer-bytes bs=1 count="$INITIAL_INPUT_SIZE"

    echo " >> Running fuzzer with failpoint"
    RUST_BACKTRACE=1 FAILPOINTS=fail_fuzzer=return ./scripts/gear.sh test fuzz "" wlogs > fuzz_run 2>&1

    echo " >> Checking fuzzer output"
    if cat fuzz_run | grep -qzP '(?s)(?=.*GasTree corrupted)(?=.*NodeAlreadyExists)(?=.*\Qpallet_gear::pallet::Pallet<T>>::consume_and_retrieve\E)' ; then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi

}

main
