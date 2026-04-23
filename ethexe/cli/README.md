# ethexe-cli

`ethexe-cli` is the command-line entrypoint for running and operating a Vara-on-Ethereum node.
It bundles four workflows behind the `ethexe` binary:

- `run` starts the full service stack from `ethexe-service`
- `key` manages the local Secp256k1 key stores used by the node and the network layer
- `tx` submits Ethereum-side and injected Vara.eth transactions
- `check` validates the local RocksDB state and can recompute announces for correctness

The crate lives in [`ethexe/cli`](.), produces the `ethexe` binary, and appends the current git
short SHA to `--version` output through [`build.rs`](./build.rs).

## Command Map

```text
ethexe [--cfg <path>|none] <COMMAND> [OPTIONS]

Commands:
  run     Start an ethexe node
  key     Manage signing and networking keys
  tx      Interact with Router and Mirror contracts
  check   Verify a local ethexe database
```

The global `--cfg` flag loads a TOML file and merges it with the command-line arguments.
CLI flags always win. If `--cfg` is omitted, the binary looks for `./.ethexe.toml`. Pass
`--cfg none` to disable config-file loading entirely.

## Configuration Model

The config file mirrors [`Params`](./src/params/mod.rs) and is split into the same five sections
the service expects:

```toml
[node]
base = "/var/lib/ethexe"
validator = "0x..."
validator-session = "0x..."
worker-threads = 8
blocking-threads = 64
chunk-processing-threads = 8
block-gas-limit = 750000000
batch-size-limit = 100
canonical-quarantine = 0
fast-sync = true

[ethereum]
rpc = "ws://127.0.0.1:8545"
beacon-rpc = "http://127.0.0.1:8545"
router = "0x..."
block-time = 12
eip1559-fee-increase-percentage = 20
blob-gas-multiplier = 2

[network]
bootnodes = ["/ip4/203.0.113.10/udp/20333/quic-v1/p2p/12D3KooW..."]
public-addr = ["/dns4/node.example.com/udp/20333/quic-v1"]
listen-addr = ["/ip4/0.0.0.0/udp/20333/quic-v1"]
port = 20333

[rpc]
port = 9944
external = false
cors = ["http://localhost:*", "http://127.0.0.1:*"]
gas_limit_multiplier = 3

[prometheus]
name = "validator-1"
port = 9635
external = false
```

Not every section is mandatory for every command:

- `run` needs `node` and `ethereum`; `network`, `rpc`, and `prometheus` are optional
- `key` only consumes the node paths used to derive default key-store locations
- `tx` reads node paths plus Ethereum connection parameters
- `check` needs the node database path and, when `--migrate` is used, Ethereum settings too

## Directory Layout

`NodeParams` derives three working directories from `node.base`:

- `db/` stores RocksDB data
- `keys/` stores the main Secp256k1 keys used by validators and transaction senders
- `net/` stores the libp2p identity used by the networking service

If `node.base` is not set, the CLI uses the platform data directory resolved from
`ProjectDirs::from("com", "Gear", "ethexe")`. When `node.tmp` or `node.dev` is enabled, the
database is moved to a temporary directory instead.

## `run`

`ethexe run` turns the merged CLI/config parameters into an `ethexe_service::config::Config`,
initializes logging, and boots the asynchronous service stack.

Development mode changes behavior intentionally:

- `Service::configure_dev_environment` starts a local Anvil-backed environment
- a validator key and validator session key are generated automatically
- the Router address and Ethereum RPC endpoints are filled in from the spawned Anvil instance
- RPC is enabled even if it is absent from the config file
- the default dev block time is one second unless overridden by `ethereum.block-time`
- canonical quarantine is disabled unless it was explicitly set

Examples:

```bash
ethexe run --dev
ethexe run --cfg /etc/ethexe.toml --verbose
ethexe run --cfg none --base ./state --ethereum-rpc ws://127.0.0.1:8545 --ethereum-router 0x...
```

## `key`

`ethexe key` is a thin wrapper around `gsigner`'s Secp256k1 CLI.
The command chooses a default storage directory before delegating to `gsigner`:

- `keys/` is used for normal validator and sender keys
- `net/` is used when `--net` is passed
- `--key-store <path>` overrides both defaults

Examples:

```bash
ethexe key keyring generate
ethexe key --net keyring list
ethexe key --key-store ./tmp-keys keyring import --secret-key <hex>
```

## `tx`

`ethexe tx` opens an Ethereum client with the selected sender key and Router address, then runs
one of the transaction-oriented subcommands.

Shared requirements:

- `--sender` is a 20-byte Ethereum address and must have a corresponding private key in the chosen key store
- `--ethereum-rpc` and `--ethereum-router` must be provided directly or through config
- `--key-store` defaults to the node `keys/` directory

Supported workflows:

- `upload` uploads a Wasm blob for Router-side validation
- `create` creates a new Mirror from a code ID, salt, and initializer
- `create-with-abi` creates a Mirror and installs an ABI interface contract address for explorers
- `query` inspects a Mirror's current state via Ethereum plus a Vara.eth RPC endpoint
- `owned-balance-top-up` adds ETH to the Mirror's owned balance
- `executable-balance-top-up` adds WVARA to the executable balance, optionally calling `approve`
- `send-message` sends either a normal Ethereum transaction or an injected Vara.eth transaction
- `send-reply` sends a reply transaction tied to a prior message
- `claim-value` claims ETH previously locked for a message
- `transfer-locked-value-to-inheritor` drains remaining locked ETH to the inheritor

The value parser accepts either raw integers or formatted currency strings:

```text
1000000000000000000
1 ETH
42 WVARA
0.5 ETH
```

Examples:

```bash
ethexe tx --sender 0x... upload ./target/wasm32-unknown-unknown/release/demo_ping.opt.wasm --watch
ethexe tx --sender 0x... create 0x... --salt 0x...
ethexe tx --sender 0x... send-message 0xMirror 0x50494e47 0 --watch
ethexe tx --sender 0x... send-message 0xMirror 0x50494e47 0 --injected --rpc-url ws://127.0.0.1:9944 --watch
ethexe tx --sender 0x... executable-balance-top-up 0xMirror "10 WVARA" --approve
```

When `--json` is supported by a subcommand, the command still prints human progress to stderr and
emits the machine-readable result on stdout.

## `check`

`ethexe check` opens a local RocksDB database and can run two families of validation:

- integrity checks walk the stored block DAG and use `ethexe_db::verifier::IntegrityVerifier`
- computation checks recompute announces with `ethexe_processor::Processor` and compare the
  resulting states, transitions, and schedule with the persisted values

Behavior notes:

- if neither `--integrity-check` nor `--computation-check` is passed, both checks run
- `--migrate` upgrades a raw database to the latest schema before validation
- `--verbose` enables debug logs and disables the progress bar
- `--db` overrides the database directory derived from `node.base`

Examples:

```bash
ethexe check --cfg /var/lib/ethexe/.ethexe.toml
ethexe check --db ./db --integrity-check
ethexe check --db ./db --computation-check --verbose
ethexe check --db ./db --migrate
```

## Source Layout

- [`src/lib.rs`](./src/lib.rs) defines the top-level CLI, config-file loading, and logging setup
- [`src/commands`](./src/commands) contains the `run`, `key`, `tx`, and `check` command handlers
- [`src/params`](./src/params) defines the merged CLI/TOML configuration model
- [`src/utils.rs`](./src/utils.rs) contains small formatting helpers used by the transaction output

If you need generated API docs as well, run:

```bash
cargo doc -p ethexe-cli --open
```
