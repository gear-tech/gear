#!/usr/bin/env sh

SELF="$0"
SCRIPTS="$(cd "$(dirname "$SELF")"/ && pwd)"

. "$SCRIPTS"/fuzzer_consts.sh

# Check platform and set grep command
if [ "$(uname)" = "Darwin" ]; then
    GREP="ggrep"  # Use ggrep on macOS
else
    GREP="grep"   # Use default grep on other systems
fi

main() {
    echo " >> Checking runtime fuzzer"
    echo " >> Getting random bytes from /dev/urandom"
    # Fuzzer expects a minimal input size of 350 KiB. Without providing a corpus of the same or larger
    # size fuzzer will stuck for a long time with trying to test the target using 0..100 bytes.
    mkdir -p utils/runtime-fuzzer/fuzz/corpus/main
    dd if=/dev/urandom of=utils/runtime-fuzzer/fuzz/corpus/main/check-fuzzer-bytes bs=1 count="$INITIAL_INPUT_SIZE"

    echo " >> Running fuzzer with failpoint"
    RUST_BACKTRACE=1 FAILPOINTS=fail_fuzzer=return ./scripts/gear.sh test fuzz "" wlogs > fuzz_run 2>&1

    echo " >> Checking fuzzer output"
    if cat fuzz_run | $GREP -qzP '(?s)(?=.*consume_and_retrieve: failed consuming the rest of gas)(?=.*NodeAlreadyExists)(?=.*pallet_gear::internal::\{impl#\d+\}::consume_and_retrieve)' ; then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi
}

main
