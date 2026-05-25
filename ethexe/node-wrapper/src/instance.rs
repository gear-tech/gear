// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::Error;
use ethexe_common::Address;
use ethexe_rpc::DevClient;
use jsonrpsee::{
    http_client::HttpClient,
    ws_client::{WsClient, WsClientBuilder},
};
use std::{net::SocketAddrV4, process::Child};

/// The Vara.eth CLI instance. Will close the instance when dropped.
///
/// Can be constructed only from [spawn_immediate](super::node::VaraEth::spawn_immediate)
/// or [spawn_ready](super::node::VaraEth::spawn_ready).
#[derive(Debug)]
pub struct VaraEthInstance {
    /// The Vara.eth rpc address.
    pub(crate) rpc_addr: SocketAddrV4,
    /// The Vara.eth anvil rpc address.
    pub(crate) eth_rpc_addr: SocketAddrV4,
    /// The child process of instance
    pub(crate) child: Child,
}

impl VaraEthInstance {
    /// Fetches the Ethereum Router address.
    pub async fn router_address(&self) -> Result<Address, Error> {
        self.http_client()?
            .router_address()
            .await
            .map_err(Error::QueryRouterAddress)
    }

    /// Returns the websocket client.
    pub async fn ws_client(&self) -> Result<WsClient, Error> {
        WsClientBuilder::new()
            .build(self.ws_endpoint())
            .await
            .map_err(Error::BuildWsClient)
    }

    /// Returns the HTTP client.
    pub fn http_client(&self) -> Result<HttpClient, Error> {
        HttpClient::builder()
            .build(self.http_endpoint())
            .map_err(Error::BuildHttpClient)
    }

    /// Returns the Websocket endpoint of Vara.eth rpc.
    pub fn ws_endpoint(&self) -> String {
        format!("ws://{}", self.rpc_addr)
    }

    /// Returns the HTTP endpoint of Vara.eth rpc.
    pub fn http_endpoint(&self) -> String {
        format!("http://{}", self.rpc_addr)
    }

    /// Returns the Websocket endpoint Vara.eth node connected to.
    pub fn ethereum_ws_endpoint(&self) -> String {
        format!("ws://{}", self.eth_rpc_addr)
    }

    /// Returns the HTTP endpoint Vara.eth node connected to.
    pub fn ethereum_http_endpoint(&self) -> String {
        format!("http://{}", self.eth_rpc_addr)
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
        let _ = self.child.kill();
    }
}
