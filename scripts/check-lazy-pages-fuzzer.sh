#!/usr/bin/env sh

SELF="$0"
SCRIPTS="$(cd "$(dirname "$SELF")"/ && pwd)"

RUN_DURATION_SECS=10
PROCESS_NAME="lazy-pages-fuzzer-runner"
OUTPUT_FILE="lazy_pages_fuzz_run"

FUZZER_INPUT_FILE=./seed.bin

main() {
    echo " >> Checking lazy pages fuzzer"
    # Remove lazy pages fuzzer run file
    rm -f $OUTPUT_FILE

    # Build lazy pages fuzzer
    LAZY_PAGES_FUZZER_ONLY_BUILD=1 ./scripts/gear.sh test lazy-pages-fuzz

    echo " >> Running lazy pages fuzzer for ${RUN_DURATION_SECS} seconds"

    # Run lazy pages fuzzer for a few seconds
    RUST_LOG="error,lazy_pages_fuzzer_runner=debug,lazy_pages_fuzzer::lazy_pages=trace" RUST_BACKTRACE=1 \
        ./scripts/gear.sh test lazy-pages-fuzz --duration-seconds ${RUN_DURATION_SECS} > $OUTPUT_FILE 2>&1

    if [ $? -ne 0 ]; then
        cat $OUTPUT_FILE
        echo "Failed to run lazy pages fuzzer"
        print_seed
        exit 1
    else
        echo "Lazy pages fuzzer run completed successfully"
    fi
}

print_seed() {
    echo "****SEED START: \""
    xxd -p $FUZZER_INPUT_FILE | tr --delete '\n'
    echo "\" *****SEED END."
}

main
