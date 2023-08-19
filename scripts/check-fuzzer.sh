#!/usr/bin/env sh

main() {
    echo " >> Getting random bytes from /dev/urandom"
    # Fuzzer expects a minimal input size of 25 MiB. Without providing a corpus of the same or larger
    # size fuzzer will stuck for a long time with trying to test the target using 0..100 bytes.
    mkdir -p utils/runtime-fuzzer/fuzz/corpus/main
    dd if=/dev/urandom of=utils/runtime-fuzzer/fuzz/corpus/main/check-fuzzer-bytes bs=1 count=27000000

    echo " >> Running fuzzer with failpoint"
    RUST_BACKTRACE=1 FAILPOINTS=fail_fuzzer=return ./scripts/gear.sh test fuzz > fuzz_run 2>&1

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
