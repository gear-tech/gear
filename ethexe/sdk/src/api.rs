// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Mirror, Router, WVara};
use anyhow::{Context, Result};
use ethexe_common::Address;
use ethexe_ethereum::{Ethereum, EthereumBuilder};
use gprimitives::ActorId;
use gsigner::secp256k1::Signer;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

pub struct VaraEthApi {
    pub(crate) vara_eth_client: WsClient,
    pub(crate) ethereum_client: Ethereum,
}

#[derive(Debug, Clone)]
pub struct VaraEthApiBuilder {
    vara_eth_rpc_url: String,
    ethereum_rpc_url: String,
    router_address: Option<Address>,
    signer: Option<Signer>,
    sender_address: Option<Address>,
    eip1559_fee_increase_percentage: Option<u64>,
    blob_gas_multiplier: Option<u128>,
}

impl VaraEthApiBuilder {
    /// Creates a builder with no configured endpoints, router address, signer, or sender address.
    ///
    /// Required fields must be set before calling [`Self::build`].
    pub fn new() -> Self {
        Self {
            vara_eth_rpc_url: String::new(),
            ethereum_rpc_url: String::new(),
            router_address: None,
            signer: None,
            sender_address: None,
            eip1559_fee_increase_percentage: None,
            blob_gas_multiplier: None,
        }
    }

    /// Sets the Vara.ETH WebSocket RPC URL.
    pub fn vara_eth_rpc_url(mut self, vara_eth_rpc_url: impl Into<String>) -> Self {
        self.vara_eth_rpc_url = vara_eth_rpc_url.into();
        self
    }

    /// Sets the Ethereum RPC URL.
    pub fn ethereum_rpc_url(mut self, ethereum_rpc_url: impl Into<String>) -> Self {
        self.ethereum_rpc_url = ethereum_rpc_url.into();
        self
    }

    /// Sets the Router contract address.
    pub fn router_address(mut self, router_address: Address) -> Self {
        self.router_address = Some(router_address);
        self
    }

    /// Sets the signer used for Ethereum transactions and injected messages.
    pub fn signer(mut self, signer: Signer) -> Self {
        self.signer = Some(signer);
        self
    }

    /// Sets the sender address used by the Ethereum client.
    pub fn sender_address(mut self, sender_address: Address) -> Self {
        self.sender_address = Some(sender_address);
        self
    }

    /// Sets the optional EIP-1559 fee increase percentage.
    pub fn eip1559_fee_increase_percentage(mut self, value: Option<u64>) -> Self {
        self.eip1559_fee_increase_percentage = value;
        self
    }

    /// Sets the optional blob gas multiplier.
    pub fn blob_gas_multiplier(mut self, value: Option<u128>) -> Self {
        self.blob_gas_multiplier = value;
        self
    }

    /// Builds an SDK client.
    pub async fn build(self) -> Result<VaraEthApi> {
        let vara_eth_rpc_url = (!self.vara_eth_rpc_url.is_empty())
            .then_some(self.vara_eth_rpc_url)
            .context("Vara.ETH RPC URL is required")?;
        let ethereum_rpc_url = (!self.ethereum_rpc_url.is_empty())
            .then_some(self.ethereum_rpc_url)
            .context("Ethereum RPC URL is required")?;
        let router_address = self.router_address.context("Router address is required")?;
        let signer = self.signer.context("signer is required")?;
        let sender_address = self.sender_address.context("sender address is required")?;

        let ethereum_client = EthereumBuilder::default()
            .rpc_url(ethereum_rpc_url)
            .router_address(router_address)
            .signer(signer)
            .sender_address(sender_address)
            .eip1559_fee_increase_percentage_opt(self.eip1559_fee_increase_percentage)
            .blob_gas_multiplier_opt(self.blob_gas_multiplier)
            .build()
            .await
            .with_context(|| "failed to create Ethereum client")?;

        VaraEthApi::new(&vara_eth_rpc_url, ethereum_client).await
    }
}

impl Default for VaraEthApiBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl VaraEthApi {
    /// Builds a new SDK client builder.
    pub fn builder() -> VaraEthApiBuilder {
        VaraEthApiBuilder::new()
    }

    pub async fn new(vara_eth_rpc_url: &str, ethereum_client: Ethereum) -> Result<Self> {
        let vara_eth_client = WsClientBuilder::new()
            .build(vara_eth_rpc_url)
            .await
            .with_context(|| "failed to create WS client for Vara.ETH RPC")?;
        Ok(Self {
            vara_eth_client,
            ethereum_client,
        })
    }

    pub(crate) fn vara_eth_client(&self) -> &WsClient {
        &self.vara_eth_client
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

    /// Returns the connected Ethereum chain id.
    pub async fn chain_id(&self) -> Result<u64> {
        self.ethereum_client.chain_id().await
    }
}
