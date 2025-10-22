// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{
    Blocks, Error, Events, TxInBlock, config::GearConfig, metadata::Event, signer::Signer,
};
use anyhow::Result;
use core::ops::{Deref, DerefMut};
use jsonrpsee::{
    client_transport::ws::{Url, WsTransportClientBuilder},
    core::client::Client,
};
use std::time::Duration;
use subxt::{
    OnlineClient,
    backend::rpc::RpcClient,
    ext::subxt_rpcs::{self, LegacyRpcMethods},
};

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc.vara.network:443";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const DEFAULT_RETRIES: u8 = 0;

const ONE_HUNDRED_MEGABYTES: u32 = 100 * 1024 * 1024;

/// Gear api wrapper.
#[derive(Clone)]
pub struct Api {
    rpc: RpcClient,
    legacy_methods: LegacyRpcMethods<GearConfig>,
    client: OnlineClient<GearConfig>,
}

impl Api {
    /// Create new API client.
    pub async fn new(uri: impl Into<Option<&str>>) -> Result<Self> {
        Self::with_timeout(uri, DEFAULT_TIMEOUT).await
    }

    pub async fn with_timeout(uri: impl Into<Option<&str>>, timeout: Duration) -> Result<Self> {
        let uri: Option<&str> = uri.into();
        let rpc_client = Self::rpc_client(uri.unwrap_or(DEFAULT_GEAR_ENDPOINT), timeout).await?;

        Self::from_rpc_client(rpc_client).await
    }

    async fn rpc_client(uri: &str, timeout: Duration) -> Result<RpcClient> {
        let url = Url::parse(uri).map_err(|_| Error::InvalidUrl)?;
        let (sender, receiver) = WsTransportClientBuilder::default()
            .max_request_size(ONE_HUNDRED_MEGABYTES)
            .connection_timeout(timeout)
            .build(url)
            .await
            .map_err(|e| jsonrpsee::core::ClientError::Transport(e.into()))
            .map_err(subxt_rpcs::Error::from)?;

        let client = Client::builder()
            .request_timeout(timeout)
            .build_with_tokio(sender, receiver);

        Ok(RpcClient::new(client))
    }

    pub async fn from_rpc_client(rpc: RpcClient) -> Result<Self> {
        let legacy_methods = LegacyRpcMethods::new(rpc.clone());
        let client = OnlineClient::from_rpc_client(rpc.clone()).await?;

        Ok(Self {
            rpc,
            client,
            legacy_methods,
        })
    }

    /// Subscribe all blocks
    ///
    ///
    /// ```ignore
    /// let api = Api::new(None).await?;
    /// let blocks = api.subscribe_blocks().await?;
    ///
    /// while let Ok(block) = blocks.next().await {
    ///   // ...
    /// }
    /// ```
    pub async fn subscribe_blocks(&self) -> Result<Blocks> {
        Ok(self.blocks().subscribe_all().await?.into())
    }

    /// Subscribe finalized blocks
    ///
    /// Same as `subscribe_blocks` but only finalized blocks.
    pub async fn subscribe_finalized_blocks(&self) -> Result<Blocks> {
        Ok(self.client.blocks().subscribe_finalized().await?.into())
    }

    /// Subscribe all events.
    ///
    /// ```ignore
    /// let api = Api::new(None).await?;
    /// let events = api.events().await?;
    ///
    /// while let Ok(evs) = events.next().await {
    ///   // ...
    /// }
    /// ```
    pub async fn events(&self) -> Result<Events> {
        Ok(self.client.blocks().subscribe_all().await?.into())
    }

    /// Parse events of an extrinsic
    pub async fn events_of(&self, tx: &TxInBlock) -> Result<Vec<Event>> {
        tx.fetch_events()
            .await?
            .iter()
            .map(|e| -> Result<Event> { e?.as_root_event::<Event>().map_err(Into::into) })
            .collect::<Result<Vec<Event>>>()
    }

    /// Subscribe finalized events
    ///
    /// Same as `events` but only finalized events.
    pub async fn finalized_events(&self) -> Result<Events> {
        Ok(self.client.blocks().subscribe_finalized().await?.into())
    }

    /// New signer from api
    pub fn signer(self, suri: &str, passwd: Option<&str>) -> Result<Signer> {
        Signer::new(self, suri, passwd).map_err(Into::into)
    }

    /// Get the underlying [`RpcClient`] instance.
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc
    }

    /// Access legacy RPC methods.
    pub fn legacy(&self) -> &LegacyRpcMethods<GearConfig> {
        &self.legacy_methods
    }
}

impl Deref for Api {
    type Target = OnlineClient<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for Api {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}
