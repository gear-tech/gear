// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Mirror, Router, WVara};
use anyhow::{Context, Result, anyhow};
use ethexe_common::Address;
use ethexe_ethereum::{Ethereum, EthereumBuilder};
use gprimitives::ActorId;
use gsigner::secp256k1::Signer;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

/// Default Ethereum RPC URL used by the underlying Ethereum client.
pub const DEFAULT_ETHEREUM_RPC: &str = Ethereum::DEFAULT_ETHEREUM_RPC;
/// Default EIP-1559 fee increase percentage for transaction fee estimation.
pub const DEFAULT_EIP1559_FEE_INCREASE_PERCENTAGE: u64 =
    Ethereum::INCREASED_EIP1559_FEE_INCREASE_PERCENTAGE;
/// Default EIP-1559 max fee per gas in gwei for transaction fee estimation.
pub const DEFAULT_EIP1559_MAX_FEE_PER_GAS_IN_GWEI: u64 =
    Ethereum::NO_EIP1559_MAX_FEE_PER_GAS_IN_GWEI as u64;
/// Default blob gas multiplier used by CLI-style transaction clients.
pub const DEFAULT_BLOB_GAS_MULTIPLIER: u64 = Ethereum::INCREASED_BLOB_GAS_MULTIPLIER as u64;

pub struct VaraEthApi {
    pub(crate) vara_eth_client: Option<WsClient>,
    pub(crate) ethereum_client: Ethereum,
}

#[derive(Debug, Clone, Default)]
pub struct VaraEthApiBuilder {
    vara_eth_rpc_url: Option<String>,
    ethereum_rpc_url: Option<String>,
    router_address: Option<Address>,
    signer: Option<Signer>,
    sender_address: Option<Address>,
    eip1559_fee_increase_percentage: Option<u64>,
    blob_gas_multiplier: Option<u128>,
}

impl VaraEthApiBuilder {
    /// Sets the Vara.ETH WebSocket RPC URL.
    pub fn vara_eth_rpc_url(mut self, vara_eth_rpc_url: impl Into<String>) -> Self {
        self.vara_eth_rpc_url = Some(vara_eth_rpc_url.into());
        self
    }

    /// Sets the Ethereum RPC URL.
    pub fn ethereum_rpc_url(mut self, ethereum_rpc_url: impl Into<String>) -> Self {
        self.ethereum_rpc_url = Some(ethereum_rpc_url.into());
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
        let ethereum_rpc_url = self
            .ethereum_rpc_url
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
            .blob_gas_multiplier(
                self.blob_gas_multiplier
                    .unwrap_or(DEFAULT_BLOB_GAS_MULTIPLIER as u128),
            )
            .build()
            .await
            .with_context(|| "failed to create Ethereum client")?;

        match self.vara_eth_rpc_url {
            Some(vara_eth_rpc_url) => VaraEthApi::new(&vara_eth_rpc_url, ethereum_client).await,
            None => Ok(VaraEthApi::from_ethereum(ethereum_client)),
        }
    }
}

impl VaraEthApi {
    /// Builds a new SDK client builder.
    pub fn builder() -> VaraEthApiBuilder {
        VaraEthApiBuilder::default()
    }

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
    /// Methods that need the Vara.ETH RPC endpoint return an error when called on this instance.
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

    /// Returns the connected Ethereum chain id.
    pub async fn chain_id(&self) -> Result<u64> {
        self.ethereum_client.chain_id().await
    }
}
