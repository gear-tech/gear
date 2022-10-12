
<p align="center">
  <a href="https://gear-tech.io">
    <img src="images/title-grey.png" width="700" alt="Gear">
  </a>
</p>

<h4 align="center">
Gear is a Substrate-based smart-contract platform allowing anyone to run dApp in a few minutes.
</h4>

#

<div align="center">

[![CI][c1]][c2]
[![GitHubStars][g1]][g2]
[![Discord][d1]][d2]
[![Twitter][t1]][t2]
[![License][l1]][l2]

[c1]: https://github.com/gear-tech/gear/workflows/CI/badge.svg
[c2]: https://github.com/gear-tech/gear/actions/workflows/CI.yaml

[g1]: https://img.shields.io/github/stars/gear-tech/gear?style=flat-square&label=Stars
[g2]: https://github.com/gear-tech/gear

[t1]: https://img.shields.io/twitter/follow/gear_techs?style=social
[t2]: https://twitter.com/gear_techs

[d1]: https://img.shields.io/discord/891063355526217738?style=flat-square&label=Discord
[d2]: https://discord.com/invite/7BQznC9uD9

[l1]: https://img.shields.io/badge/License-GPL%203.0-success
[l2]: https://github.com/gear-tech/gear/blob/master/LICENSE
</div>

<p align="center">Hit the <a href="https://github.com/gear-tech/gear">:star:</a> button to keep up with our daily progress!</p>

# Getting Started

1. :open_hands: The easiest way to get started with Gear is to use a demo environment on [https://idea.gear-tech.io](https://idea.gear-tech.io).

2. :wrench: Follow the instructions from ["Getting started in 5 minutes"](https://wiki.gear-tech.io/getting-started-in-5-minutes) to compile the Rust test smart contract to WebAssembly. :running: Upload and run smart contract in Gear demo environment on [https://idea.gear-tech.io](https://idea.gear-tech.io), send a message to a program, check how it is going.

3. :scroll: Write your own smart contract or take one from the [examples](https://github.com/gear-dapps). A comprehensive amount of smart contract examples are available for your convenience and faster onboarding.

4. :computer: Download and run your Gear node locally or create your own multi-node local testnet.

5. :dolphin: Deep dive to the [Smart Contracts section](https://wiki.gear-tech.io/developing-contracts/gear-program) of the Gear Wiki for more details about how to implement and run your dApp in Gear.

## Run Gear Node

Gear node can run in a single Dev Net mode or you can create a Multi-Node local testnet or make your own build of Gear node.

1. Compile and launch node as described in [Gear Node README](https://github.com/gear-tech/gear/tree/master/node/README.md). Alternatively, download nightly build of Gear node:

    - **Windows x64**: [gear-nightly-windows-x86_64.zip](https://get.gear.rs/gear-nightly-windows-x86_64.zip)
    - **macOS M-series (ARM)**: [gear-nightly-macos-m.tar.gz](https://get.gear.rs/gear-nightly-macos-m.tar.gz)
    - **macOS Intel x64**: [gear-nightly-macos-x86_64.tar.gz](https://get.gear.rs/gear-nightly-macos-x86_64.tar.gz)
    - **Linux x64**: [gear-nightly-linux-x86_64.tar.xz](https://get.gear.rs/gear-nightly-linux-x86_64.tar.xz)

2. Run Gear node without special arguments to get a node connected to the testnet:

    ```bash
    gear
    ```

3. One may run a local node in development mode for testing purposes. This node will not be connected to any external network. Use `--dev` argument for running the node locally and `--tmp` for storing the state in temporary storage:

    ```bash
    gear --dev --tmp
    ```

4. Get more info about usage details, flags, available options and subcommands:

    ```bash
    gear --help
    ```

## Implement and run your own blockchain application

1. Gear provides dApp [application templates](https://github.com/gear-dapps) that cover various use cases - DeFi, DAO, NFT and more. Write your own smart contract or take one from the available templates. Adapt a template in accordance to your business needs.

2. Test your smart contract off-chain, test it on-chain using a local node, then upload to Gear network.

3. Implement an interface for your dApp for interaction Gear network using [JS API](https://github.com/gear-tech/gear-js/tree/main/api) or use provided by Gear on [https://idea.gear-tech.io](https://idea.gear-tech.io).


# Gear components

* [core](https://github.com/gear-tech/gear/tree/master/core) - Gear engine for distributed computing core components.

* [node](https://github.com/gear-tech/gear/tree/master/node) - Gear substrate-based node, ready for hacking :rocket:.

* [gstd](https://github.com/gear-tech/gear/tree/master/gstd) - Standard library for Gear smart contracts.

* [gear-js](https://github.com/gear-tech/gear-js/tree/main/api) - jsonrpc API of Gear backend.

* [examples](https://github.com/gear-dapps) - Gear smart contract examples.

Go to https://docs.gear.rs to dive into the documentation on Gear crates.

# What does Gear do?

<p align="center">
<img src="images/rust.png" height="64"><br>Gear provides the easiest and most cost-effective way <br>to run WebAssembly programs (smart-contracts) compiled from <br>many popular languages, such as Rust, C/C++ and more.
</p>
<p align="center">
<img src="images/api.png" height="64"><br>Gear ensures very minimal, intuitive, and sufficient API <br>for running both newly written and existing programs <br>on multiple networks without the need to rewrite them.
</p>
<p align="center">
<img src="images/state.png" height="64"><br>Smart Contracts are stored in the blockchain’s state <br>and are invoked preserving their state upon request.
</p>
<p align="center">
<img src="images/apps.png" height="64"><br>Gear enables a seamless transition to Web3, <br>enabling the running of dApps, microservices, middleware and open APIs.
</p>

### :fire: Key features

 - Programs run in WASM VM (near-native code execution speed)
 - **Unique** :crown: : Parallelizable architecture (even greater speed)
 - **Unique** :crown: : Actor model for message-passing communications - secure, effective, clear
 - dApp in minutes using Gear libraries
 - Based on Substrate

### Main capabilities

Gear enables anyone to create and run any custom-logic dApp and is a go-to solution for the following types of applications:
  - **Run dApps** that support business logic of any project in the **decentralized Gear network** (very fast). Upload programs to the network and interact with them.
  - Being a **Polkadot parachain**, Gear establishes cross-chain communications between other blockchains, allowing anyone to run a dApp in the Polkadot network in a very **cost-less** manner.
  - Join Substrate-supported blockchains in any other platform outside Polkadot.
  - A standalone instance running microservices, middleware, open API and more

  # Why?

The blockchain technology launched a rapid transition from centralized, server-based internet (Web2) to decentralized, distributed one (Web3).

Web3 introduces a new type of decentralized applications (dApps) that enable the existence of DeFi, DEX, Decentralized marketplaces, NFTs, Creators and Social Tokens.

Smart Contract is an equivalent of a microservice which is stored on the blockchain network and is the essential building block of a decentralized application.

Modern blockchains solve many issues of the older blockchain networks, such as:
 - Lack of scalability, low transaction speed, high transaction costs
 - Domain-specific development language (high barrier to entry)
 - Complex and inefficient native consensus protocols
 - Absence of intercommunication tools

But still have room for improvements due to:
 - Fixated, rigid native consensus protocols
 - Lack of interoperability with other networks

To resolve the interoperability issue, Parity technologies focused on creating a technology that connects every other blockchain:
  - Polkadot - a blockchain of blockchains. Provides a “relay chain” (the primary blockchain) that enables “parachains” (functional blockchains) to be deployed on top of it. All parachains are interconnected, creating a massive network of multifunctional blockchain services.
  - Substrate - a modular framework that allows to create custom-built blockchains with consensus mechanism, core functionality and security out of the box.

Building a blockchain with Substrate allows it to be deployed on any compatible relay chain such as Polkadot and Kusama
Substrate serves as a layer of communication between the relay chain and the parachain

# How does it work?

The internal flow of Gear:

  <img src="images/internal_flow.jpg" alt="Snow" style="width:100%;">

Refer to the <a href="https://github.com/gear-tech/gear-technical/blob/master/TECHNICAL.pdf">technical paper</a> for some insights about how Gear works internally.

# Performance

Performance charts can be seen here: https://gear-tech.github.io/performance-charts.

# Contribution

You can request a new feature by creating a new Issue or discuss it with us on [Discord](https://discord.gg/7BQznC9uD9).
Here are some features in-prog or planned: https://github.com/gear-tech/gear/issues

# License

Gear is licensed under [GPL v3.0 with a classpath linking exception](LICENSE).

##

<h4>
<p align="left" nowrap>
    <a href="https://twitter.com/gear_techs">
        <img src="./images/social-icon-1.svg" alt="twit" style="vertical-align:middle" >
    </a>
    <a href="https://github.com/gear-tech">
        <img src="./images/social-icon-2.svg" alt="github" style="vertical-align:middle" >
    </a>
    <a href="https://discord.gg/7BQznC9uD9">
        <img src="./images/social-icon-3.svg" alt="discord" style="vertical-align:middle" >
    </a>
    <a href="https://medium.com/@gear_techs">
        <img src="./images/social-icon-4.svg" alt="medium" style="vertical-align:middle" >
    </a>
    <a href="https://t.me/gear_tech">
        <img src="./images/social-icon-5.svg" alt="medium" style="vertical-align:middle" >
   </a>
    <br>
    <a href="https://wiki.gear-tech.io/" nowrap>
       Wiki
    </a> •
    <a href="https://gear-tech.io/#community">
      Community
    </a> •
    <a href="https://gear-tech.io/events.html">
      Events
    </a> •
    <a href="https://gear-tech.io/#about">
      About us
    </a>
</p>
</h4>
