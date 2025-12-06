
<p align="center">
  <a href="https://gear-tech.io">
    <img src="images/title-grey.png" width="700" alt="Gear">
  </a>
</p>

<h3 align="center">
Gear Protocol is a Substrate-based platform for developers, enabling anyone to spin up a dApp in just a few minutes.
</h3>

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

<p align="center">Hit the <a href="https://github.com/gear-tech/gear">:star:</a> button to keep up with the daily protocol development progress!</p>

# Overview

Gear Protocol provides a developer-friendly programming platform for decentralized applications, along with custom runtime technology that can be used to deploy Layer-1 networks for running applications in a decentralized manner. The vision for Gear is to empower developers to create and deploy next-generation Web3.0 applications in the easiest and most efficient way possible.

## :fire: Key Features

- **Unique** :crown: : The main idea underpinning the Gear Protocol is the Actor model for message communications - secure, effective, clear.
- **Unique** :crown: : Parallelizable architecture ensures even greater speed.
- **Unique** :crown: : Continued messaging automation through delayed messages enables truly on-chain dApps.
- **Unique** :crown: : Built-in Actors to provide programs with enhanced access to pallets and to offload high-load computations off-chain.
- **Unique** :crown: : Create a dApp in minutes using Gear Protocol's libraries.
- Programs run in a Wasm VM, enabling near-native code execution speed.
- Based on Substrate, Gear Protocol ensures fork-less upgrades and compatibility with other blockchains.

## Capabilities

- Gear Protocol provides dApp developers with a very minimal, intuitive, and sufficient API for writing custom-logic programs in Rust and running them on Gear-powered networks, such as the Vara Network.
- It provides a technological foundation for constructing highly scalable and rapid decentralized Layer-1 networks.
- Reduces the computational burden on blockchains by offloading highly intensive calculations using a Vara node with Wasm VM, and then proving the correctness of these calculations on any blockchain.
- A Vara node can be used as a standalone instance running microservices, middleware, open API, and more.

For more details refer to the **[Gear Whitepaper](https://whitepaper.gear.foundation)**.

Refer to the **[Technical Paper](https://github.com/gear-tech/gear-technical/blob/master/TECHNICAL.pdf)** for some insights about how it works internally.

# Getting Started

1. :book: Visit **[Gear Wiki](https://wiki.gear-tech.io/)** to get all the details about how to start implementing your own blockchain application.
    1. Follow the instructions from ["Getting started in 5 minutes"](https://wiki.gear-tech.io/docs/getting-started-in-5-minutes/) to compile your first Rust test program to Wasm.
    2. Upload and run the program on the Vara Network Testnet via **[Gear Idea](https://idea.gear-tech.io/programs?node=wss%3A%2F%2Ftestnet.vara.network)**, send a message to a program and read the program's state.
2. :scroll: Write your own program or take one from the comprehensive [examples library](https://wiki.gear-tech.io/docs/examples/prerequisites) as a basis for a convenient and swift onboarding process.
    1. Explore dApp examples in action and gain a deeper understanding of their functionalities. Write your own program or use one from the available templates. Adapt a template according to your business needs.
    2. [Test](https://wiki.gear-tech.io/docs/developing-contracts/testing) your program off-chain and on-chain using a [local node](https://wiki.gear-tech.io/docs/node/setting-up).
    3. Then upload it via Gear Idea to the [Vara Network](https://idea.gear-tech.io/programs?node=wss%3A%2F%2Frpc.vara.network).
3. :microscope: Dive into the documentation on Gear Protocol crates at [сrates.io](https://crates.io/teams/github:gear-tech:dev). Particular attention should be paid to - [sails_rs](https://crates.io/crates/sails_rs), [gstd](https://crates.io/crates/gstd), [gcore](https://crates.io/crates/gcore), [gtest](https://crates.io/crates/gtest), [gsdk](https://crates.io/crates/gsdk). More details can be found in the Documentation section for each crate.

4. :iphone: Implement a frontend application that interacts with your program using the [JS API](https://github.com/gear-tech/gear-js/tree/main/api). React application examples are available [here](https://github.com/gear-foundation/dapps/tree/master/frontend/apps).

# Run Vara Node

### Get the binaries

To build Vara node binaries from source follow the step-by-step instructions provided in [Node README](https://github.com/gear-tech/gear/tree/master/node/README.md).

Alternatively, you can download pre-built packages for your OS/architecture:

  - **macOS M-series (ARM)**: [gear-nightly-aarch64-apple-darwin.tar.xz](https://get.gear.rs/gear-nightly-aarch64-apple-darwin.tar.xz)
  - **macOS Intel x64**: [gear-nightly-x86_64-apple-darwin.tar.xz](https://get.gear.rs/gear-nightly-x86_64-apple-darwin.tar.xz)
  - **Linux x64**: [gear-nightly-x86_64-unknown-linux-gnu.tar.xz](https://get.gear.rs/gear-nightly-x86_64-unknown-linux-gnu.tar.xz)
  - **Windows x64**: [gear-nightly-x86_64-pc-windows-msvc.zip](https://get.gear.rs/gear-nightly-x86_64-pc-windows-msvc.zip)


### Run Vara Dev network locally

Running the following command will start a single-node Vara Dev net with two users - Alice and Bob:

  ```bash
  gear --dev
  ```

# Performance

Performance charts can be seen here: https://gear-tech.github.io/performance-charts.

# Contribution

You can request a new feature by [creating a new issue](https://github.com/gear-tech/gear/issues/new/choose) or discuss it with us on [Discord](https://discord.gg/7BQznC9uD9).
Here are some features in progress or planned: https://github.com/gear-tech/gear/issues

# License

Gear Protocol is licensed under [GPL v3.0 with a classpath linking exception](LICENSE).

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
    <br> •
    <a href="https://gear-tech.io">
      About us
    </a> •
    <a href="https://wiki.gear-tech.io/" nowrap>
      Gear Wiki
    </a> •
    <a href="https://gear.foundation/news">
      News
    </a> •
      <a href="https://gear.foundation/events">
      Events
    </a> •
    <a href="https://vara.network/">
      Vara Network
    </a> •
</p>
</h4>
