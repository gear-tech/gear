// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Vara.eth Node Wrapper
//!
//! A test and development harness that spawns a local `Vara.eth` (`ethexe`) node as a child
//! OS process and exposes its RPC endpoints to the caller.
//!
//! It locates the `ethexe` binary, runs it in `--dev` mode (which starts an internal Anvil
//! instance and deploys the required Ethereum contracts), and returns a handle to the node's
//! JSON-RPC and backing Ethereum RPC endpoints. On drop, [`VaraEthInstance`] tears down the
//! whole process group, including the internal Anvil process.
//!
//! Consumed by `ethexe-sdk`. Depends on [`ethexe-rpc`] (client feature) for the JSON-RPC
//! client traits and on `ethexe-common` for shared address types.
//!
//! ## Public API
//!
//! | Item | Description |
//! |------|-------------|
//! | [`VaraEth`] | Builder: configure binary path, block time, RPC port, startup timeout, and extra args, then call [`VaraEth::spawn_immediate`] or [`VaraEth::spawn_ready`]. |
//! | [`VaraEthInstance`] | Handle to a running node, exposing `router_address`, `ws_client`, `http_client`, and the WS/HTTP endpoints for both the node and its Ethereum RPC. Closes the node on drop. |
//! | [`Error`] | Error enum covering binary lookup, spawn, startup timeout, client construction, and router-address query failures. |
//!
//! ## Key Invariants
//!
//! - RPC is always enabled; the default node port is `9944` and the Ethereum RPC endpoint is
//!   `127.0.0.1:8545` (the Anvil default used by `ethexe run --dev`).
//! - [`VaraEth::spawn_ready`] returns [`Error::Timeout`] if the node does not answer within the
//!   configured startup timeout (default 5 s); [`VaraEth::spawn_immediate`] returns at once.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use ethexe_node_wrapper::VaraEth;
//!
//! async fn do_some_stuff() {
//!     let veth = VaraEth::new().spawn_ready().await.unwrap();
//!
//!     let http_endpoint = veth.http_endpoint();
//!     let router = veth.router_address().await.unwrap();
//!
//!     println!("Vara.eth running at: {http_endpoint}");
//!     println!("Router address: {router}");
//!     // `veth` drops here — node and its internal Anvil are shut down.
//! }
//! ```

#![warn(missing_docs, unreachable_pub)]

/// Crate errors module.
pub mod error;
pub use error::Error;

/// The node wrapper spawned instance module
pub mod instance;
pub use instance::VaraEthInstance;

/// The node wrapper configuration module.
pub mod node;
pub use node::VaraEth;
