// This file is part of Gear.
//
// Copyright (C) 2025-2026 Gear Technologies Inc.
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
//! [`promise_manager::PromiseSubscriptionManager`] is the main entity
//! responsible for promise handling. It maintains single-promise
//! subscribers indexed by transaction hash and dispatches the matching
//! [`ethexe_common::injected::SignedPromise`] to the right subscriber
//! when one arrives via [`server::InjectedApi::send_promise`].
//!
//! Subscribers are spawned via [`spawner::spawn_pending_subscriber`].
//! The pending subscriber is dropped after waiting for `20 * slot`
//! seconds to avoid stuck subscribers.

pub(crate) mod promise_manager;
pub(crate) mod relay;
pub(crate) mod server;
pub(crate) mod spawner;

mod r#trait;

pub use server::InjectedApi;
pub use r#trait::InjectedServer;

#[cfg(feature = "client")]
pub use r#trait::InjectedClient;
