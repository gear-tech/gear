// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{Mirror, Router, WVara};
use anyhow::{Context, Result};
use ethexe_ethereum::Ethereum;
use gprimitives::ActorId;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

/// Root handle for the ethexe SDK, combining an ethexe JSON-RPC WebSocket client with an
/// Ethereum contract client.
///
/// Construct with [`VaraEthApi::new`], then obtain scoped wrappers via [`VaraEthApi::mirror`],
/// [`VaraEthApi::router`], or [`VaraEthApi::wrapped_vara`]. The struct borrows no external state
/// and can be shared across tasks by wrapping in `Arc`.
pub struct VaraEthApi {
    pub(crate) vara_eth_client: WsClient,
    pub(crate) ethereum_client: Ethereum,
}

impl VaraEthApi {
    /// Creates a new `VaraEthApi` by connecting a WebSocket JSON-RPC client to `vara_eth_rpc_url`
    /// and storing the provided `ethereum_client`.
    ///
    /// Returns an error if the WebSocket connection cannot be established.
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

    /// Returns a [`Mirror`] scoped to the on-chain program identified by `actor_id`.
    ///
    /// The returned wrapper borrows `self` and provides per-program operations such as sending
    /// messages and querying state.
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

    /// Returns a [`Router`] wrapper that exposes Router contract operations and global queries.
    pub fn router(&self) -> Router<'_> {
        let router_client = self.ethereum_client.router();
        let router_query_client = router_client.query();
        Router {
            api: self,
            router_client,
            router_query_client,
        }
    }

    /// Returns a [`WVara`] wrapper for the WrappedVara ERC-20 contract operations.
    pub fn wrapped_vara(&self) -> WVara {
        let wvara_client = self.ethereum_client.wrapped_vara();
        let wvara_query_client = wvara_client.query();
        WVara {
            wvara_client,
            wvara_query_client,
        }
    }
}
