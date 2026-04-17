// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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

pub(crate) mod promise_manager;

pub(crate) mod relay;

pub(crate) mod server;
pub use server::InjectedApi;

pub(crate) mod spawner;

mod r#trait;
pub use r#trait::InjectedServer;

#[cfg(feature = "client")]
pub use r#trait::InjectedClient;
