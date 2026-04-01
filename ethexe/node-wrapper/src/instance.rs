// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

use anyhow::{Context, Result};
use ethexe_common::Address;
use ethexe_rpc::DevClient;
use jsonrpsee::{
    http_client::HttpClient,
    ws_client::{WsClient, WsClientBuilder},
};
use std::{net::SocketAddrV4, process::Child};

pub struct InstanceConfig {
    pub ethereum_rpc: SocketAddrV4,
    pub rpc_addr: SocketAddrV4,
}

pub struct VaraEthInstance {
    /// The Vara.eth node instance configuration.
    pub(crate) config: InstanceConfig,
    /// The child process of instance
    pub(crate) child: Child,
}

impl VaraEthInstance {
    /// Fetches the Ethereum Router address.
    pub async fn router_address(&self) -> Result<Address> {
        self.http_client()?
            .router_address()
            .await
            .with_context(|| "failed to query router address")
    }

    pub async fn ws_client(&self) -> Result<WsClient> {
        WsClientBuilder::new()
            .build(self.ws_endpoint())
            .await
            .with_context(|| "failed to build ws client")
    }

    pub fn http_client(&self) -> Result<HttpClient> {
        HttpClient::builder()
            .build(self.http_endpoint())
            .with_context(|| "failed to build HttpClient")
    }

    /// Returns the Websocket endpoint of Vara.eth rpc.
    pub fn ws_endpoint(&self) -> String {
        format!("ws://{}", self.config.rpc_addr)
    }

    /// Returns the HTTP endpoint of Vara.eth rpc.
    pub fn http_endpoint(&self) -> String {
        format!("http://{}", self.config.rpc_addr)
    }

    /// Returns the Websocket endpoint Vara.eth node connected to.
    pub fn ethereum_rpc_ws_endpoint(&self) -> String {
        format!("ws://{}", self.config.ethereum_rpc)
    }

    /// Returns the HTTP endpoint Vara.eth node connected to.
    pub fn ethereum_rpc_http_endpoint(&self) -> String {
        format!("http://{}", self.config.ethereum_rpc)
    }
}

impl Drop for VaraEthInstance {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            // Here is hack for unix systems - kill all processes in group to force anvil drop.
            let pid = self.child.id() as i32;
            unsafe { libc::kill(-pid, libc::SIGTERM) };
        }

        #[cfg(not(unix))]
        self.child.kill();
    }
}
