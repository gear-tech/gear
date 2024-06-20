// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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
    client::Rpc, config::GearConfig, metadata::Event, signer::Signer, AsOption, Blocks, Events,
    Result, TxInBlock,
};
use core::ops::{Deref, DerefMut};
use std::result::Result as StdResult;
use subxt::{Error, OnlineClient};

/// Gear api wrapper.
#[derive(Clone)]
pub struct Api {
    /// How many times we'll retry when rpc requests failed.
    pub retry: u16,
    client: OnlineClient<GearConfig>,

    /// Gear RPC client
    rpc: Rpc,
}

impl Api {
    /// Create new API client.
    pub async fn new(url: impl AsOption<str>) -> Result<Self> {
        Self::new_with_timeout(url, None).await
    }

    /// Gear RPC Client
    pub fn rpc(&self) -> Rpc {
        self.rpc.clone()
    }

    /// Create new API client with timeout.
    pub async fn new_with_timeout(
        url: impl AsOption<str>,
        timeout: impl AsOption<u64>,
    ) -> Result<Self> {
        let rpc = Rpc::new(url, timeout).await?;

        Ok(Self {
            // Retry our failed RPC requests for 5 times by default.
            retry: 5,
            client: OnlineClient::from_rpc_client(rpc.client()).await?,
            rpc,
        })
    }

    /// Setup retry times and return the API instance.
    pub fn with_retry(mut self, retry: u16) -> Self {
        self.retry = retry;
        self
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
            .map(|e| -> StdResult<Event, Error> { e?.as_root_event::<Event>() })
            .collect::<StdResult<Vec<Event>, Error>>()
            .map_err(Into::into)
    }

    /// Subscribe finalized events
    ///
    /// Same as `events` but only finalized events.
    pub async fn finalized_events(&self) -> Result<Events> {
        Ok(self.client.blocks().subscribe_finalized().await?.into())
    }

    /// New signer from api
    pub fn signer(self, suri: &str, passwd: Option<&str>) -> Result<Signer> {
        Signer::new(self, suri, passwd)
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
