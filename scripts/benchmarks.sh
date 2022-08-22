#!/bin/bash

steps=50
repeat=20

output=./runtime/parachain-gear/src/weights

chain=local

pallets=(
    frame_system
    pallet_balances
    pallet_session
    pallet_timestamp
    pallet_utility
    pallet_collator_selection
    cumulus_pallet_xcmp_queue
)

for p in ${pallets[@]}
do
    ./target/release/gear-node benchmark pallet \
        --chain=$chain \
        --execution=wasm \
        --wasm-execution=compiled \
        --pallet=$p  \
        --extrinsic='*' \
        --steps=$steps  \
        --repeat=$repeat \
        --json-file=./bench-gear.json \
        --header=./scripts/file_header.txt \
        --output=$output
done
