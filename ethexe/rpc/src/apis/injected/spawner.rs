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

use super::promise_manager::PendingSubscriber;
use ethexe_common::{HashOf, injected::InjectedTransaction};
use jsonrpsee::{SubscriptionMessage, SubscriptionSink};
use tracing::{trace, warn};

/// Spawns [PendingSubscriber] in tokio runtime.
///
/// On task finishing applies the `on_finish` function that is need to drop some data.
pub fn spawn_pending_subscriber<F>(
    sink: SubscriptionSink,
    subscriber: PendingSubscriber,
    on_finish: F,
) where
    F: FnOnce(HashOf<InjectedTransaction>) + std::marker::Send + 'static,
{
    let (tx_hash, receiver) = subscriber.into_parts();

    // TODO: think about using this handle for aborting runtime tasks in case of long waiting.
    let _handle = tokio::spawn(async move {
        let _guard = scopeguard::guard(tx_hash, on_finish);

        // Waiting for the first one: promise, timeout_err, client disconnect error.
        let promise = tokio::select! {
            result = receiver => match result {
                Ok(promise_result) => match promise_result {
                    Ok(promise) => promise,
                    Err(_err) => {
                        unreachable!("promise sender is owned by the server; it cannot be dropped before this point");
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

        // TODO: remove unwrap here
        let message = SubscriptionMessage::from_json(&promise).unwrap();
        if let Err(err) = sink.send(message).await {
            trace!("failed to send promise, client disconnected: err={err}");
        }
    });
}
