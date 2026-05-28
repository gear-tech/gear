// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Vara.eth node wrapper.
//!
//! Node wrapper is a wrapper around the Vara.eth node.
//! Internally, it do the next things:
//! - spawns the Vara.eth node process
//!     - spawns the Anvil node process
//!     - deploy Ethereum smart-contracts on Anvil
//! - provides the access to node PRC endpoints and Ethereum RPC endpoint
//!
//! ## Modules
//! - [`node`] - provides the [VaraEth] struct - the node configurator
//! - [`instance`] - provides the [VaraEthInstance] struct - the instance which holds the inner spawned process and RPC endpoints.
//! - [`error`] - provides the [Error] enum - the error type for module errors.

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
