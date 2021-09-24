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

Gear is a new Polkadot/Kusama parachain and most advanced L2 smart-contract engine allowing anyone to launch any dApp for networks with untrusted code.

Gear provides the easiest and most cost-effective way to run WebAssembly programs (smart-contracts) compiled from many popular languages, such as C/C++, Rust and more.

Gear ensures very minimal, intuitive, and sufficient API for running both newly written and existing programs on multiple networks without the need to rewrite them.

Refer to the [technical paper](https://github.com/gear-tech/gear-technical/blob/master/TECHNICAL.pdf) for some insights about how Gear works internally.

## Getting Started

1. To start familiarity with Gear, download and run Gear node connected to the testnet.

2. Deploy and test smart contracts, check how it is going. A comprehensive amount of smart contract examples is available for your convenience and faster onboarding.

## Run Gear Node

1. Download nightly build of Gear node:

    - **Windows x64**: [gear-nightly-windows-x86_64.zip](https://builds.gear.rs/gear-nightly-windows-x86_64.zip)
    - **macOS M1**: [gear-nightly-macos-m1.tar.gz](https://builds.gear.rs/gear-nightly-macos-m1.tar.gz)
    - **macOS Intel x64**: [gear-nightly-macos-x86_64.tar.gz](https://builds.gear.rs/gear-nightly-macos-x86_64.tar.gz)
    - **Linux x64**: [gear-nightly-linux-x86_64.tar.xz](https://builds.gear.rs/gear-nightly-linux-x86_64.tar.xz)

2. Run Gear node without special arguments to get a node connected to the testnet:

    ```bash
    gear-node
    ```

3. Get more info about usage details, flags, avilable options and subcommands:

    ```bash
    gear-node --help
    ```

Gear node can run in a single Dev Net mode or you can create a Multi-Node local testnet or make your own build of Gear node.

Refer to the [Gear Node README](https://github.com/gear-tech/gear/tree/master/node) for details and some examples.

## Run you first smart contract

Prepare your program
1. Choose an example program from [examples](https://github.com/gear-tech/gear/tree/master/examples) or use your own program
2. Follow the [instruction](https://github.com/gear-tech/gear/blob/master/examples/README.md) to build the Rust to WebAssembly program
    
### Upload program from UI
    
1. Go to [https://idea.gear-tech.io](https://idea.gear-tech.io)

2. Sign in to Gear via Telegram or Github account

3. Connect to your node:
    1. TBD

4. Click **Upload program** and choose compiled .wasm file to upload
    1. Set enough Gas limit - 1 000 000
    2. Intial parameters, Initial value - keep default
    3. Upload file with Metadata, if applicable
        1. the file can be found after program compilation from available examples
    4. Click **Upload program**, wait for the program initialization

6. Try sending message to program, check reply:
    1. TBD... 

### Upload program locally from cmd
    
Upload program locally on your node: 

TBD...

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
