<br/>

<p align="center">
  <a href="https://gear-tech.io">
    <img src="https://gear-tech.io/images/logo-black.svg" width="240" alt="GEAR">
  </a>
</p>

<p align="center">
  <b>Computational Component of Polkadot Network</b>
</p>

<p align=center>
    <a href="https://github.com/gear-tech/gear/actions/workflows/master.yml"><img src="https://github.com/gear-tech/gear/workflows/CI/badge.svg"></a>
    <a href="https://github.com/gear-tech/gear/blob/master/LICENSE"><img src="https://img.shields.io/badge/License-GPL%203.0-success"></a>
</p>

<br/>

<p>Gear is a new Polkadot/Kusama parachain and most advanced L2 smart-contract engine allowing anyone to launch any dApp for networks with untrusted code.</p>
<p>Gear provides the easiest and most cost-effective way to run WebAssembly programs (smart-contracts) compiled from many popular languages, such as C/C++, Rust and more.</p>
<p>Gear ensures very minimal, intuitive and sufficient API for running both - newly written and existing programs on other networks without the need to rewrite them.</p>

## Getting Started

Please refer to the [technical paper](https://github.com/gear-tech/gear-technical/blob/master/TECHNICAL.pdf) for some insights about how Gear works internally.

**TODO**: *Describe the easiest way to start with. Pay attention to smart contract examples.*

## Running Node

**TODO**: *Prepare ready-to-install packages to make first steps simpler.*

### Prerequisites

1. Install Rust using [rustup](https://rustup.rs/):

    ```bash
    curl https://sh.rustup.rs -sSf | sh
    ```

2. Add toolchains:

    ```bash
    make init
    ```

### Build Gear Node and Run

1. Build:

    ```bash
    make node
    ```

2. Run:

    ```bash
    make node-run
    ```

Refer to the [Gear Node](https://github.com/gear-tech/gear/tree/master/node) docs for details.

## Gear Components

* [core](https://github.com/gear-tech/gear/tree/master/core)

    Gear engine for distributed computing core components.

* [node](https://github.com/gear-tech/gear/tree/master/node)

    Gear substrate-based node, ready for hacking :rocket:.

* [gstd](https://github.com/gear-tech/gear/tree/master/gstd)

    Standard library for Gear smart contracts.

* [examples](https://github.com/gear-tech/gear/tree/master/examples)

    Gear smart contract examples.

## License

Gear is licensed under [GPL v3.0 with a classpath linking exception](LICENSE).
