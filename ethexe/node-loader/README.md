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

## Value control for testnet and mainnet

`load` mode now supports optional value control. If you do not pass any value flags, the
loader keeps its current dev-oriented behavior.

Available flags:

- `--value-profile <dev|testnet|mainnet>`
- `--max-msg-value <u128>` in wei
- `--max-top-up-value <u128>` in WVARA smallest units
- `--total-msg-value-budget <u128>` in wei
- `--total-top-up-budget <u128>` in WVARA smallest units

Safe testnet example:

```bash
./target/debug/ethexe-node-loader load --value-profile testnet --workers 1 --batch-size 1
```

Safe mainnet example with explicit overrides:

```bash
./target/debug/ethexe-node-loader load \
  --value-profile mainnet \
  --max-msg-value 50000000000000 \
  --total-msg-value-budget 1000000000000000 \
  --max-top-up-value 500000000000 \
  --total-top-up-budget 5000000000000 \
  --workers 1 \
  --batch-size 1
```

Budget accounting is based on planned spend at scheduling time. The batch that crosses a
budget is still submitted, then the loader stops scheduling new work and exits with
`status: budget exhausted`.

## What it does

Runs a continuous load test against an `ethexe` dev node, generating randomized batches that:

- upload code/programs,
- send messages and replies,
- claim values,

to stress-test the runtime and networking stack.

It derives worker accounts from the standard Anvil mnemonic, funds them, and then runs batch workers in parallel. It also subscribes to new blocks to drive batch scheduling.
