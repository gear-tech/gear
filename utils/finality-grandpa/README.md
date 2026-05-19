# finality-grandpa

[![crates.io link][crates-badge]][crates] [![Build Status](https://github.com/paritytech/finality-grandpa/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/paritytech/finality-grandpa/actions/workflows/rust.yml) [![Coverage Status](https://coveralls.io/repos/github/paritytech/finality-grandpa/badge.svg?branch=master)](https://coveralls.io/github/paritytech/finality-grandpa?branch=master)

<img align="right" width="150" height="150" src="img/grandpa.png">

**GRANDPA**, **G**HOST-based **R**ecursive **AN**cestor **D**eriving **P**refix **A**greement, is a
finality gadget for blockchains, implemented in Rust. It allows a set of nodes to come to BFT
agreement on what is the canonical chain, which is produced by some external block production
mechanism. It works under the assumption of a partially synchronous network model and with the
presence of up to 1/3 Byzantine nodes.

## Build & Test

The only dependency required to build and run the tests is to have a stable version of Rust
installed.

```
git clone https://github.com/paritytech/finality-grandpa
cd finality-grandpa
cargo build
cargo test
```

## Usage

Add this to your Cargo.toml:

```toml
[dependencies]
finality-grandpa = "0.16"
```

**Features:**

- `derive-codec` - Derive `Decode`/`Encode` instances of [parity-scale-codec][parity-scale-codec]
  for all the protocol messages.
- `test-helpers` - Expose some opaque types for testing purposes.

### Integration

This crate only implements the state machine for the GRANDPA protocol. In order to use this crate it
is necessary to implement some traits to do the integration which are responsible for providing
access to the underlying blockchain and setting up all the network communication.

#### [`Chain`][chain-docs]

The `Chain` trait allows the GRANDPA voter to check ancestry of a given block and also to query the
best block in a given chain (which will be used for voting on).

#### [`Environment`][environment-docs]

The `Environment` trait defines the types that will be used for the input and output stream to
receive and broadcast messages. It is also responsible for setting these up for a given round
(through `round_data`), as well as timers which are used for timeouts in the protocol.

The trait exposes callbacks for the full lifecycle of a round:

- proposed
- prevoted
- precommitted
- completed

As well as callbacks for notifying about block finality and voter misbehavior (equivocations).

### Substrate

The main user of this crate is [Substrate][substrate] and should be the main resource used to look
into how the integration is done. The [`substrate-finality-grandpa` crate][substrate-finality-grandpa]
should have most of the relevant integration code.

Most importantly this crate does not deal with authority set changes. It assumes that the set of
authorities is always the same. Authority set handoffs are handled in Substrate by listening to
signals emitted on the underlying blockchain.

### Fuzzing

To run the fuzzing test harness you need to install either `afl` or `cargo-fuzz` (you'll need a nightly Rust toolchain
for this):

```sh
cargo install cargo-fuzz
cargo install afl
```

#### libfuzzer

```sh
cargo fuzz run graph
cargo fuzz run round
```

#### afl

```sh
cd fuzz
cargo afl build --features afl --bin graph_afl
cargo afl build --features afl --bin round_afl

# create some random input
mkdir afl_in && dd if=/dev/urandom of=afl_in/seed bs=1024 count=4

cargo afl fuzz -i afl_in -o afl_out target/debug/graph_afl
cargo afl fuzz -i afl_in -o afl_out target/debug/round_afl
```

## Resources

- [Paper][paper]
- [Introductory Blogpost][blogpost]
- [Polkadot Wiki][polkadot-wiki]
- [Sub0 Presentation][sub0]
- [Testnet][testnet]

## License

Usage is provided under the Apache License (Version 2.0). See [LICENSE](LICENSE) for the full
details.

[blogpost]: https://medium.com/polkadot-network/grandpa-block-finality-in-polkadot-an-introduction-part-1-d08a24a021b5
[chain-docs]: https://docs.rs/finality-grandpa/latest/finality_grandpa/trait.Chain.html
[codecov-badge]: https://codecov.io/gh/paritytech/finality-grandpa/branch/master/graph/badge.svg
[codecov]: https://codecov.io/gh/paritytech/finality-grandpa
[crates-badge]: https://img.shields.io/crates/v/finality-grandpa.svg
[crates]: https://crates.io/crates/finality-grandpa
[environment-docs]: https://docs.rs/finality-grandpa/latest/finality_grandpa/voter/trait.Environment.html
[paper]: https://github.com/w3f/consensus/blob/master/pdf/grandpa.pdf
[parity-scale-codec]: https://github.com/paritytech/parity-scale-codec
[polkadot-wiki]: https://wiki.polkadot.network/en/latest/polkadot/learn/consensus/
[sub0]: https://www.youtube.com/watch?v=QE8svRKVYOU
[substrate]: https://github.com/paritytech/substrate
[substrate-finality-grandpa]: https://github.com/paritytech/substrate/tree/master/client/finality-grandpa
[testnet]: https://telemetry.polkadot.io/#/Alexander
[travis-badge]: https://travis-ci.org/paritytech/finality-grandpa.svg?branch=master
[travis]: https://travis-ci.org/paritytech/finality-grandpa
