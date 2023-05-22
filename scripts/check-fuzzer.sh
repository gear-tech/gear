#!/usr/bin/env sh

main() {
    echo " >> Running fuzzer with failpoint"
    FAILPOINTS=fail_fuzzer=return ./scripts/gear.sh test fuzz > fuzz_run 2>&1


    if cat fuzz_run | grep -q -P '^(?=.*GasTree corrupted)(?=.*NodeWasConsumed)' ; then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi
}

main
