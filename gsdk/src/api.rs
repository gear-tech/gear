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
    Blocks, Events, ProgramStateChanges, Result, TxInBlock, UserMessageSentFilter,
    UserMessageSentSubscription, config::GearConfig, gear::Event, signer::Signer,
};
use core::ops::{Deref, DerefMut};
use jsonrpsee::{client_transport::ws::Url, ws_client::WsClientBuilder};
use sp_core::H256;
use std::{borrow::Cow, time::Duration};
use subxt::{
    OnlineClient,
    backend::rpc::RpcClient,
    ext::subxt_rpcs::{LegacyRpcMethods, rpc_params},
};

const ONE_HUNDRED_MEGABYTES: u32 = 100 * 1024 * 1024;

/// Gear api wrapper.
#[derive(Debug, Clone)]
pub struct Api {
    rpc: RpcClient,
    legacy_methods: LegacyRpcMethods<GearConfig>,
    client: OnlineClient<GearConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct ApiBuilder<'a> {
    uri: Option<Cow<'a, str>>,
    timeout: Option<Duration>,
}

impl Api {
    /// Default API endpoint.
    pub const DEFAULT_ENDPOINT: &str = "wss://rpc.vara.network:443";

    /// Default timeout duration.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

    /// Creates an [`Api`] builder.
    pub const fn builder<'a>() -> ApiBuilder<'a> {
        ApiBuilder {
            uri: None,
            timeout: None,
        }
    }
}

impl<'a> ApiBuilder<'a> {
    pub fn uri(mut self, uri: impl Into<Cow<'a, str>>) -> Self {
        self.uri = Some(uri.into());
        self
    }

    pub const fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub async fn build(self) -> Result<Api> {
        Api::from_rpc_client(self.rpc_client().await?).await
    }

    async fn rpc_client(self) -> Result<RpcClient> {
        let uri = self.uri.as_ref().map_or(Api::DEFAULT_ENDPOINT, Cow::as_ref);
        let uri = Url::parse(uri)?;

        let timeout = self.timeout.unwrap_or(Api::DEFAULT_TIMEOUT);

        let client = WsClientBuilder::new()
            .max_request_size(ONE_HUNDRED_MEGABYTES)
            .connection_timeout(timeout)
            .request_timeout(timeout)
            .build(uri)
            .await?;

        Ok(RpcClient::new(client))
    }
}

impl Api {
    /// Constructs an instance of [`Self`].
    pub async fn new(uri: &str) -> Result<Self> {
        Self::builder().uri(uri).build().await
    }

    /// Construcs an instance of [`Api`] from [`RpcClient`].
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

    /// Subscribe to program state changes reported.
    pub async fn subscribe_program_state_changes(
        &self,
        program_ids: Option<Vec<H256>>,
    ) -> Result<ProgramStateChanges> {
        let subscription = self
            .rpc()
            .subscribe(
                "gear_subscribeProgramStateChanges",
                rpc_params![program_ids],
                "gear_unsubscribeProgramStateChanges",
            )
            .await?;

        Ok(ProgramStateChanges::new(subscription))
    }

    /// Subscribe to user message notifications.
    pub async fn subscribe_user_message_sent(
        &self,
        filter: UserMessageSentFilter,
    ) -> Result<UserMessageSentSubscription> {
        let subscription = self
            .rpc()
            .subscribe(
                "gear_subscribeUserMessageSent",
                rpc_params![filter],
                "gear_unsubscribeUserMessageSent",
            )
            .await?;

        Ok(UserMessageSentSubscription::new(subscription))
    }

    /// New signer from api
    pub fn signer(self, suri: &str, passwd: Option<&str>) -> Result<Signer> {
        Signer::new(self, suri, passwd)
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
