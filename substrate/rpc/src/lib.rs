// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate RPC implementation.
//!
//! A core implementation of Substrate RPC interfaces.

#![warn(missing_docs)]

pub use jsonrpsee::core::id_providers::{
    RandomIntegerIdProvider as RandomIntegerSubscriptionId,
    RandomStringIdProvider as RandomStringSubscriptionId,
};
pub use sc_rpc_api::DenyUnsafe;

pub mod author;
pub mod chain;
pub mod dev;
pub mod mixnet;
pub mod offchain;
pub mod state;
pub mod statement;
pub mod system;
pub mod utils;

#[cfg(any(test, feature = "test-helpers"))]
pub mod testing;

/// Task executor that is being used by RPC subscriptions.
pub type SubscriptionTaskExecutor = std::sync::Arc<dyn sp_core::traits::SpawnNamed>;
