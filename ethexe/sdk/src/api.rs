// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Mirror, Router, WVara};
use anyhow::{Context, Result, anyhow};
use ethexe_ethereum::Ethereum;
use gprimitives::ActorId;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

pub struct VaraEthApi {
    pub(crate) vara_eth_client: Option<WsClient>,
    pub(crate) ethereum_client: Ethereum,
}

impl VaraEthApi {
    /// Builds an SDK client with both Ethereum contract access and Vara.ETH RPC access.
    pub async fn new(vara_eth_rpc_url: &str, ethereum_client: Ethereum) -> Result<Self> {
        let vara_eth_client = WsClientBuilder::new()
            .build(vara_eth_rpc_url)
            .await
            .with_context(|| "failed to create WS client for Vara.ETH RPC")?;
        Ok(Self {
            vara_eth_client: Some(vara_eth_client),
            ethereum_client,
        })
    }

    /// Builds an SDK client for Ethereum contract access only.
    ///
    /// Methods that need the Vara.ETH RPC endpoint, such as injected transactions and state
    /// reads, return an error when called on this instance.
    pub fn from_ethereum(ethereum_client: Ethereum) -> Self {
        Self {
            vara_eth_client: None,
            ethereum_client,
        }
    }

    pub(crate) fn vara_eth_client(&self) -> Result<&WsClient> {
        self.vara_eth_client
            .as_ref()
            .ok_or_else(|| anyhow!("Vara.ETH RPC client is not configured for this SDK instance"))
    }

    pub fn mirror(&self, actor_id: ActorId) -> Mirror<'_> {
        let mirror_client = self
            .ethereum_client
            .mirror(actor_id.to_address_lossy().into());
        let mirror_query_client = mirror_client.query();
        Mirror {
            api: self,
            mirror_client,
            mirror_query_client,
        }
    }

    pub fn router(&self) -> Router<'_> {
        let router_client = self.ethereum_client.router();
        let router_query_client = router_client.query();
        Router {
            api: self,
            router_client,
            router_query_client,
        }
    }

    pub fn wrapped_vara(&self) -> WVara {
        let wvara_client = self.ethereum_client.wrapped_vara();
        let wvara_query_client = wvara_client.query();
        WVara {
            wvara_client,
            wvara_query_client,
        }
    }
}
