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
    transports::{TransportError, TransportResult},
};
use nonempty::NonEmpty;
use std::collections::VecDeque;
use tokio::sync::Mutex;

/// [`FallbackWs`] is a wrapper around [`WsConnect`].
/// It implements [`PubSubConnect`] trait to manage the ws connection in case when they dropped.
pub struct FallbackWs {
    rpc: Mutex<VecDeque<WsConnect>>,
}

impl FallbackWs {
    pub fn new(rpc: NonEmpty<String>) -> Self {
        let rpc = Mutex::new(rpc.into_iter().map(WsConnect::new).collect());
        Self { rpc }
    }

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
    pub async fn client(rpc: NonEmpty<String>) -> Result<RpcClient, TransportError> {
        ClientBuilder::default().pubsub(Self::new(rpc)).await
    }

    /// Method returns the next web socket connection.
    /// It takes one [`WsConnect`] out of the deque and push a clone back to keep rotation.
    async fn next_ws(&self) -> WsConnect {
        let mut rpc = self.rpc.lock().await;
        // safe because `rpc` was constructed from a NonEmpty collection
        let ws = rpc.pop_front().expect("rpc contains at least one element");
        rpc.push_back(ws.clone());
        ws
    }
}

impl PubSubConnect for FallbackWs {
    fn is_local(&self) -> bool {
        false
    }

    async fn connect(&self) -> TransportResult<ConnectionHandle> {
        self.next_ws().await.connect().await
    }

    async fn try_reconnect(&self) -> TransportResult<ConnectionHandle> {
        self.next_ws().await.connect().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::{
        node_bindings::Anvil,
        providers::{Provider, ProviderBuilder, ext::AnvilApi},
    };

    #[tokio::test]
    async fn test_drop_anvil() {
        gear_utils::init_default_logger();

        let anvil = Anvil::new().spawn();
        let anvil2 = Anvil::new().spawn();

        let rpc = nonempty::nonempty![anvil.ws_endpoint(), anvil2.ws_endpoint()];
        let fallback_client = FallbackWs::client(rpc).await.unwrap();

        let provider = ProviderBuilder::new().connect_client(fallback_client);

        provider.anvil_mine(Some(1000), None).await.unwrap();

        let block1 = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(block1.header.number, 1000);

        drop(anvil);

        let block2 = provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            block2.header.number, 0,
            "Expect block2 received from second rpc"
        );
    }
}
