// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate RPC interfaces.
//!
//! A collection of RPC methods and subscriptions supported by all substrate clients.

#![warn(missing_docs)]

mod error;
mod policy;

pub use policy::{check_if_safe, DenyUnsafe, UnsafeRpcError};

pub mod author;
pub mod chain;
pub mod child_state;
pub mod dev;
pub mod mixnet;
pub mod offchain;
pub mod state;
pub mod statement;
pub mod system;
