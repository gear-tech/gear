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
    Blocks, Events, TxInBlock, client::Rpc, config::GearConfig, metadata::Event, signer::Signer,
};
use anyhow::Result;
use core::ops::{Deref, DerefMut};
use subxt::OnlineClient;

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc.vara.network:443";
const DEFAULT_TIMEOUT_MILLISECS: u64 = 60_000;
const DEFAULT_RETRIES: u8 = 0;

/// Gear api wrapper.
#[derive(Clone)]
pub struct Api {
    /// Substrate client
    client: OnlineClient<GearConfig>,

    /// Gear RPC client
    rpc: Rpc,
}

impl Api {
    /// Create new API client.
    pub async fn new(uri: impl Into<Option<&str>>) -> Result<Self> {
        Self::builder().build(uri).await
    }

    /// Resolve api builder
    pub fn builder() -> ApiBuilder {
        ApiBuilder::default()
    }

    /// Gear RPC Client
    pub fn rpc(&self) -> Rpc {
        self.rpc.clone()
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
        Ok(self.client.blocks().subscribe_all().await?.into())
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

/// gsdk api builder
pub struct ApiBuilder {
    /// RPC retries
    retries: u8,
    /// RPC timeout
    timeout: u64,
}

impl ApiBuilder {
    /// Build api from the provided config
    pub async fn build(self, uri: impl Into<Option<&str>>) -> Result<Api> {
        let uri: Option<&str> = uri.into();
        let rpc = Rpc::new(
            uri.unwrap_or(DEFAULT_GEAR_ENDPOINT),
            self.timeout,
            self.retries,
        )
        .await?;

        Ok(Api {
            client: OnlineClient::from_rpc_client(rpc.client()).await?,
            rpc,
        })
    }

    /// Set rpc retries
    pub fn retries(mut self, retries: u8) -> Self {
        self.retries = retries;
        self
    }

    /// Set rpc timeout in milliseconds
    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for ApiBuilder {
    fn default() -> Self {
        Self {
            retries: DEFAULT_RETRIES,
            timeout: DEFAULT_TIMEOUT_MILLISECS,
        }
    }
}
