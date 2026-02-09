# ethexe-node-loader

Small load-testing tool for an `ethexe` dev node.

## Quick start

Build the binaries first (both are required):

- Build `ethexe-cli` (and the workspace): run `cargo build -p ethexe-cli`.
- Build `ethexe-node-loader`: run `cargo build -p ethexe-node-loader`.

Start the local dev node with:

- `ethexe run --dev --pre-funded-accounts 48 --block-time 12`

Then run the loader with:

- `RUST_LOG=debug,alloy_transport=off,alloy_provider=off,alloy_rpc_client=off,alloy_json_rpc=off,alloy_pubsub=off ./target/debug/ethexe-node-loader load --batch-size 4 --workers 16`

## What it does

Runs a continuous load test against an `ethexe` dev node, generating randomized batches that:

- upload code/programs,
- send messages and replies,
- claim values,

to stress-test the runtime and networking stack.

It derives worker accounts from the standard Anvil mnemonic, funds them, and then runs batch workers in parallel. It also subscribes to new blocks to drive batch scheduling.
