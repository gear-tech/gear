#!/usr/bin/env sh

SELF="$0"
SCRIPTS="$(cd "$(dirname "$SELF")"/ && pwd)"

. "$SCRIPTS"/fuzzer_consts.sh

RUN_DURATION_SECS=10
PROCESS_NAME="lazy-pages-fuzzer-fuzz"
OUTPUT_FILE="lazy_pages_fuzz_run"

main() {
    echo " >> Checking lazy pages fuzzer"
    echo " >> Getting random bytes from /dev/urandom"
    # Fuzzer expects a minimal input size of 350 KiB. Without providing a corpus of the same or larger
    # size fuzzer will stuck for a long time with trying to test the target using 0..100 bytes.
    mkdir -p utils/lazy-pages-fuzzer/fuzz/corpus/main
    dd if=/dev/urandom of=utils/runtime-fuzzer/fuzz/corpus/main/check-fuzzer-bytes bs=1 count="$INITIAL_INPUT_SIZE"

    # Remove lazy pages fuzzer run file
    rm -f $OUTPUT_FILE

    # Build lazy pages fuzzer
    LAZY_PAGES_FUZZER_ONLY_BUILD=1 ./scripts/gear.sh test lazy-pages-fuzz

    echo " >> Running lazy pages fuzzer for ${RUN_DURATION_SECS} seconds"

    # Run lazy pages fuzzer for a few seconds
    ( RUST_LOG="error,lazy_pages_fuzzer::lazy_pages=trace" RUST_BACKTRACE=1 ./scripts/gear.sh test lazy-pages-fuzz "" > $OUTPUT_FILE 2>&1 ) & \
        sleep ${RUN_DURATION_SECS} ; \
        kill -s KILL $(pidof $PROCESS_NAME) 2> /dev/null ; \
        echo " >> Lazy pages fuzzer run completed" ;

    # Trim output after SIGKILL backtrace
    OUTPUT=$(sed '/SIGKILL/,$d' $OUTPUT_FILE)

    if echo $OUTPUT | grep -q 'SIG: Unprotect WASM memory at address' && \
        ! echo $OUTPUT | grep -iq "ERROR"
    then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi
}

main
