// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # RPC Server Injected API
//!
//! ## Promises Flow
//! [promise_manager::PromiseSubscriptionManager] is the main entity that is responsible for
//! promises handling.
//! Internally it maintains single-promise subscribers.
//!
//! After the manager successfully registers a subscriber for
//! [ethexe_common::injected::SignedPromise], it creates the
//! [promise_manager::PendingSubscriber] and spawns it using
//! [spawner::spawn_pending_subscriber].
//!
//! **Important:** the pending subscriber will be dropped after
//! waiting for **20 * Ethereum slot** seconds to avoid dead subscribers.
//!
//! [promise_manager::PromiseSubscriptionManager] provides two methods for receiving promises:
//! - [promise_manager::PromiseSubscriptionManager::on_compact_promise] receives the promise
//!   signature from the producer. If it matches a promise already stored in the database, it is
//!   sent to the subscriber.
//! - [promise_manager::PromiseSubscriptionManager::on_computed_promise] receives the promise
//!   body. When RPC receives the corresponding promise signature, it sends the signed promise to
//!   the subscriber.

#[cfg(feature = "server")]
pub(crate) mod promise_manager;

#[cfg(feature = "server")]
pub(crate) mod relay;

#[cfg(feature = "server")]
pub(crate) mod server;
#[cfg(feature = "server")]
pub use server::InjectedApi;

#[cfg(feature = "server")]
pub(crate) mod spawner;

mod r#trait;
#[cfg(feature = "server")]
pub use r#trait::InjectedServer;

#[cfg(feature = "client")]
pub use r#trait::InjectedClient;
