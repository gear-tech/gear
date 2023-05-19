#!/usr/bin/env sh

main() {
    cargo +nightly build -p gear-common -F fuzz
    CONSUME_WITH_LOCK_MUTATION=$(jq 'select(.mutation.fn_name=="consume") | select(.mutation.mutator=="unop_not") | select(.mutation.location_in_file=="542:12-542:13") | .id' target/mutagen/mutations)
    echo " >> Running fuzzer check with mutation id $CONSUME_WITH_LOCK_MUTATION"
    MUTATION_ID=$CONSUME_WITH_LOCK_MUTATION ./scripts/gear.sh test fuzz > fuzz_run 2>&1


    if cat fuzz_run | grep -q -P '^(?=.*GasTree corrupted)(?=.*ConsumedWithLock)' ; then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi
}

main
