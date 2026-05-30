// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Vara.eth Node Wrapper
//!
//! A test and development harness that spawns a local `Vara.eth` (`ethexe`) node as a child
//! OS process and exposes its RPC endpoints to the caller.
//!
//! ## Purpose
//!
//! This crate locates the `ethexe` binary on `$PATH` (or a caller-supplied path), spawns it
//! with `["run", "--dev", "--no-network"]`, and returns a handle that gives access to the
//! node's JSON-RPC and backing Ethereum RPC endpoints. The `--dev` flag causes the node to
//! start an internal Anvil instance and deploy the required Ethereum smart-contracts; this
//! crate does not deploy contracts directly.
//!
//! On drop, [`VaraEthInstance`] sends `SIGTERM` to the entire process group so the internally-
//! spawned Anvil process is also torn down.
//!
//! ## Role in the Stack
//!
//! ```text
//! ethexe-sdk
//!     └── ethexe-node-wrapper   (spawns & manages child process)
//!             └── ethexe binary (ethexe-cli / ethexe-service)
//!                     └── Anvil (Ethereum dev node, started by --dev flag)
//! ```
//!
//! This crate depends on [`ethexe-rpc`] (client feature) for the JSON-RPC client traits used
//! to query the running node and on `ethexe-common` for shared address types.
//!
//! ## Public API
//!
//! | Item | Description |
//! |------|-------------|
//! | [`VaraEth`] | Builder: configure binary path, block time, RPC port, startup timeout, extra args, then call `spawn_immediate` or `spawn_ready`. |
//! | [`VaraEthInstance`] | Handle to a running node. Provides `router_address`, `ws_client`, `http_client`, `ws_endpoint`, `http_endpoint`, `ethereum_ws_endpoint`, `ethereum_http_endpoint`. Closes the node on drop. |
//! | [`Error`] | `thiserror` error enum covering `BinaryNotFound`, `Spawn`, `Timeout`, `BuildHttpClient`, `BuildWsClient`, `QueryRouterAddress`. |
//!
//! ## Key Invariants
//!
//! - RPC is always enabled; default port is `9944`.
//! - The Ethereum RPC endpoint is always `127.0.0.1:8545` (the Anvil default used by
//!   `ethexe run --dev`).
//! - `spawn_ready` polls the WebSocket endpoint and returns [`Error::Timeout`] if the node
//!   does not answer within the configured startup timeout (default 5 s).
//! - The child process is placed in its own process group on spawn so that the drop-time group
//!   kill reaches the internal Anvil process.
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
