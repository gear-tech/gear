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
    client::Rpc, config::GearConfig, metadata::Event, signer::Signer, Blocks, Events, TxInBlock,
};
use anyhow::Result;
use core::ops::{Deref, DerefMut};
use subxt::OnlineClient;

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
    pub async fn new(url: impl Into<Option<&str>>) -> Result<Self> {
        Self::new_with_timeout(url.into(), None).await
    }

    /// Gear RPC Client
    pub fn rpc(&self) -> Rpc {
        self.rpc.clone()
    }

    /// Create new API client with timeout.
    pub async fn new_with_timeout(
        url: impl Into<Option<&str>>,
        timeout: impl Into<Option<u64>>,
    ) -> Result<Self> {
        let rpc = Rpc::new(url, timeout, 3).await?;

        Ok(Self {
            client: OnlineClient::from_rpc_client(rpc.client()).await?,
            rpc,
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
