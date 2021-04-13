# Gear Node

Gear substrate-based node, ready for hacking :rocket:

## Dev Net

At the MVP stage, dev net is supported!

to run a dev net, compile and run `gear-node`:

```bash
cargo build --release
./target/release/gear-node --dev
```

Then follow to https://polkadot.js.org/ and connect to a local dev node.

How to program your gear node, see `EXAMPLES.md`

Purge any existing dev chain state:

```bash
./target/release/gear-nnode purge-chain --dev
```

Start a dev chain with detailed logging:

```bash
RUST_LOG=debug RUST_BACKTRACE=1 ./target/release/gear-node -lruntime=debug --dev
```

### Multi-Node Local Testnet

At the MVP stage, multinode testnet is also supported!

To see the multi-node consensus algorithm in action, run a local testnet with two validator nodes,
Alice and Bob, that have been [configured](./node/src/chain_spec.rs) as the initial
authorities of the `local` testnet chain and endowed with testnet units.

Note: this will require two terminal sessions (one for each node).

Start Alice's node first. The command below uses the default TCP port (30333) and specifies
`/tmp/alice` as the chain database location. Alice's node ID will be
`12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp` (legacy representation:
`QmRpheLN4JWdAnY7HGJfWFNbfkQCb6tFf4vvA6hgjMZKrR`); this is determined by the `node-key`.

```bash
cargo run -- \
  --base-path /tmp/alice \
  --chain=local \
  --alice \
  --node-key 0000000000000000000000000000000000000000000000000000000000000001 \
  --telemetry-url 'ws://telemetry.polkadot.io:1024 0' \
  --validator
```

In another terminal, use the following command to start Bob's node on a different TCP port (30334)
and with a chain database location of `/tmp/bob`. The `--bootnodes` option will connect his node to
Alice's on TCP port 30333:

```bash
cargo run -- \
  --base-path /tmp/bob \
  --bootnodes /ip4/127.0.0.1/tcp/30333/p2p/12D3KooWEyoppNCUx8Yx66oV9fJnriXwCcXwDDUA2kj6vnc6iDEp \
  --chain=local \
  --bob \
  --port 30334 \
  --ws-port 9945 \
  --telemetry-url 'ws://telemetry.polkadot.io:1024 0' \
  --validator
```

Execute `cargo run -- --help` to learn more about the template node's CLI options.

# License

See `LICENSE`
