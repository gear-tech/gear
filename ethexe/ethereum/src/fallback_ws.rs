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

use alloy::{
    providers::WsConnect,
    pubsub::{ConnectionHandle, PubSubConnect},
    rpc::client::{ClientBuilder, RpcClient},
    transports::{RpcError, TransportError, TransportErrorKind, TransportResult},
};
use anyhow::anyhow;
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// Builds the [`RpcClient`] for using [`alloy::providers::Provider`]
/// with inner [`FallbackWs`] pubsub connect implementation.
///
/// ## Usage example:
/// ```rust ignore
/// let main_ws = "wss://";
/// // Fallback  ws
/// let public_ws = "wss://infura.io.public.rpc/...";
///
/// let client = rpc_client_with_fallback(String::new(), vec![String::new()])
///     .await
///     .unwrap();
/// let provider = ProviderBuilder::default().connect_client(client);
/// ```
pub async fn rpc_client_with_fallback(
    current: String,
    fallbacks: Vec<String>,
) -> Result<RpcClient, TransportError> {
    let fallbacks: VecDeque<_> = fallbacks.into_iter().map(WsConnect::new).collect();

    let fallback_ws = FallbackWs {
        current: Mutex::new(WsConnect::new(current)),
        fallbacks: Mutex::new(fallbacks),
    };
    ClientBuilder::default().pubsub(fallback_ws).await
}

/// [`FallbackWs`] is a wrapper around [`WsConnect`].
/// It implements [`PubSubConnect`] trait to manage the ws connection in case when they dropped.
pub struct FallbackWs {
    current: Mutex<WsConnect>,
    fallbacks: Mutex<VecDeque<WsConnect>>,
}

impl PubSubConnect for FallbackWs {
    fn is_local(&self) -> bool {
        false
    }

    async fn connect(&self) -> TransportResult<ConnectionHandle> {
        self.current.lock().await.connect().await
    }

    async fn try_reconnect(&self) -> TransportResult<ConnectionHandle> {
        let mut current = self.current.lock().await;
        let mut fallbacks = self.fallbacks.lock().await;

        // stores drop ws connections
        let mut dropped_connections = Vec::new();
        dropped_connections.push(current.clone());

        loop {
            let next_ws = fallbacks.pop_front().ok_or_else(|| {
                RpcError::Transport(TransportErrorKind::Custom(
                    anyhow!("all ws connections dropped, verify the network").into(),
                ))
            })?;

            match next_ws.connect().await {
                Ok(conn) => {
                    tracing::trace!(
                        previous_connection = %current.url(),
                        new_connection = %next_ws.url(),
                        "reconnecting to new web socket"
                    );
                    // Add dropped connections to fallbacks, assume that in future they may be valid
                    fallbacks.extend(dropped_connections);
                    *current = next_ws;
                    return Ok(conn);
                }
                Err(err) => {
                    tracing::trace!(
                        ws = %next_ws.url(),
                        err = %err,
                        "failed to connect to ws"
                    );
                    dropped_connections.push(next_ws);
                    continue;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::{
        node_bindings::Anvil,
        providers::{Provider, ProviderBuilder},
    };

    #[tokio::test]
    async fn test_drop_anvil() {
        gear_utils::init_default_logger();

        let anvil = Anvil::new().block_time(1).spawn();
        let anvil2 = Anvil::new().block_time_f64(0.0001).spawn();

        // sleep for some time to allow anvil mine some blocks
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let client = rpc_client_with_fallback(anvil.ws_endpoint(), vec![anvil2.ws_endpoint()])
            .await
            .unwrap();

        let provider = ProviderBuilder::new().connect_client(client);

        let block1 = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .unwrap()
            .unwrap();
        tracing::debug!("receive block: {}", block1.header.number);

        drop(anvil);

        let block2 = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .unwrap()
            .unwrap();
        tracing::debug!("receive second block: {}", block2.header.number);

        // Verify, that block2 was received from second rpc
        assert!(block2.header.number - block1.header.number > 100);
    }
}
