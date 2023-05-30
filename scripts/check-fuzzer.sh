#!/usr/bin/env sh

main() {
    echo " >> Running fuzzer with failpoint"
    RUST_BACKTRACE=1 FAILPOINTS=fail_fuzzer=return ./scripts/gear.sh test fuzz > fuzz_run 2>&1

    if cat fuzz_run | grep -qzP '(?s)(?=.*GasTree corrupted)(?=.*NodeAlreadyExists)(?=.*\Qpallet_gear::pallet::Pallet<T>>::consume_and_retrieve\E)' ; then
        echo "Success"
        exit 0
    else
        echo "Failed"
        exit 1
    fi

}

main
