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

//! Spawns a tokio task that bridges a [`PendingSubscriber`] to a
//! jsonrpsee subscription sink.

use super::promise_manager::PendingSubscriber;
use ethexe_common::{HashOf, injected::InjectedTransaction};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use tracing::{error, trace, warn};

/// Spawns the subscriber bridge. `on_finish` runs once the task exits
/// (timeout, client disconnect, or successful delivery) — typically a
/// [`PromiseSubscriptionManager::cancel_registration`] call to clean
/// the subscriber map.
pub fn spawn_pending_subscriber<F>(
    sink: SubscriptionSink,
    subscriber: PendingSubscriber,
    on_finish: F,
) where
    F: FnOnce(HashOf<InjectedTransaction>) + Send + 'static,
{
    let (tx_hash, receiver) = subscriber.into_parts();

    tokio::spawn(async move {
        let _guard = scopeguard::guard(tx_hash, on_finish);

        let promise = tokio::select! {
            result = receiver => match result {
                Ok(promise_result) => match promise_result {
                    Ok(promise) => promise,
                    Err(_err) => {
                        unreachable!(
                            "promise sender is owned by the server; \
                             it cannot be dropped before this point"
                        );
                    }
                },
                Err(_) => {
                    warn!("promise wasn't received in time, finish waiting");
                    return;
                }
            },
            _ = sink.closed() => {
                trace!("subscription closed by user, stop background task");
                return;
            }
        };

        match SubscriptionMessage::from_json(&promise) {
            Ok(message) => {
                if let Err(err) = sink.send(message).await {
                    trace!(
                        ?promise,
                        ?err,
                        "failed to send promise, client disconnected",
                    );
                }
            }
            Err(err) => {
                error!(
                    ?promise,
                    ?err,
                    "serialization error: failed to create `SubscriptionMessage` from promise; \
                     this must never happen"
                );
            }
        }
    });
}
