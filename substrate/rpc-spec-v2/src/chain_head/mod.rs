// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate chain head API.
//!
//! # Note
//!
//! Methods are prefixed by `chainHead`.

#[cfg(test)]
pub(crate) mod test_utils;
#[cfg(test)]
mod tests;

pub mod api;
pub mod chain_head;
pub mod error;
pub mod event;

mod chain_head_follow;
mod chain_head_storage;
mod subscription;

pub use api::ChainHeadApiServer;
pub use chain_head::{ChainHead, ChainHeadConfig};
pub use event::{
    BestBlockChanged, ErrorEvent, Finalized, FollowEvent, Initialized, NewBlock, RuntimeEvent,
    RuntimeVersionEvent,
};
