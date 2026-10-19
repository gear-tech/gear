// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate chain specification API.
//!
//! The *chain spec* (short for *chain specification*) allows inspecting the content of
//! the specification of the chain that a JSON-RPC server is targeting.
//!
//! The values returned by the API are guaranteed to never change during the lifetime of the
//! JSON-RPC server.
//!
//! # Note
//!
//! Methods are prefixed by `chainSpec`.

#[cfg(test)]
mod tests;

pub mod api;
pub mod chain_spec;

pub use api::ChainSpecApiServer;
pub use chain_spec::ChainSpec;
