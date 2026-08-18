// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Substrate Client and associated logic.
//!
//! The [`Client`] is one of the most important components of Substrate. It mainly comprises two
//! parts:
//!
//! - A database containing the blocks and chain state, generally referred to as
//!   the [`Backend`](sc_client_api::backend::Backend).
//! - A runtime environment, generally referred to as the
//!   [`Executor`](sc_client_api::call_executor::CallExecutor).
//!
//! # Initialization
//!
//! Creating a [`Client`] is done by calling the `new` method and passing to it a
//! [`Backend`](sc_client_api::backend::Backend) and an
//! [`Executor`](sc_client_api::call_executor::CallExecutor).
//!
//! The former is typically provided by the `sc-client-db` crate.
//!
//! The latter typically requires passing one of:
//!
//! - A [`LocalCallExecutor`] running the runtime locally.
//! - A `RemoteCallExecutor` that will ask a third-party to perform the executions.
//! - A `RemoteOrLocalCallExecutor` combination of the two.
//!
//! Additionally, the fourth generic parameter of the `Client` is a marker type representing
//! the ways in which the runtime can interface with the outside. Any code that builds a `Client`
//! is responsible for putting the right marker.

mod block_rules;
mod call_executor;
mod client;
mod code_provider;
mod notification_pinning;
mod wasm_override;
mod wasm_substitutes;

pub use call_executor::LocalCallExecutor;
pub use client::{Client, ClientConfig};
pub(crate) use code_provider::CodeProvider;

#[cfg(feature = "test-helpers")]
pub use self::client::{new_in_mem, new_with_backend};
