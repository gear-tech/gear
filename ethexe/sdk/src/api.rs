// This file is part of Gear.

// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use crate::{Mirror, Router, WVara};
use anyhow::{Context, Result};
use ethexe_ethereum::Ethereum;
use gprimitives::ActorId;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};

pub struct VaraEthApi {
    pub(crate) vara_eth_client: WsClient,
    pub(crate) ethereum_client: Ethereum,
}

impl VaraEthApi {
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
