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

## Help

Use the built-in CLI help:

- `./target/debug/ethexe-node-loader --help`
