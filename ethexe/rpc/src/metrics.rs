// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Metrics for the RPC server.

use metrics::{Counter, Gauge};

// TODO kuzmindev: add metrics for all RPC apis, e.g number of calls, latency, errors, etc.

/// Metrics for the Injected RPC API.
#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_rpc_injected_api")]
pub struct InjectedApiMetrics {
    /// The number of calls to `injected_sendTransaction`.
    pub send_injected_tx_calls: Counter,
    /// The number of calls to `injected_subscribeTransactionPromise`.
    pub send_and_watch_injected_tx_calls: Counter,
    /// The number of active injected transaction promises subscriptions.
    pub injected_tx_active_subscriptions: Gauge,
    /// The total number of injected transaction promises given to subscribers.
    pub injected_tx_promises_given: Counter,
}
