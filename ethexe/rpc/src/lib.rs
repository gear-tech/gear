// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

pub use crate::apis::InjectedTransactionAcceptance;

#[cfg(feature = "client")]
pub use crate::apis::{BlockClient, CodeClient, InjectedClient, ProgramClient};

use anyhow::Result;
use apis::{
    BlockApi, BlockServer, CodeApi, CodeServer, InjectedApi, InjectedServer, ProgramApi,
    ProgramServer,
};
use ethexe_common::injected::{RpcOrNetworkInjectedTx, SignedPromise};
use ethexe_db::Database;
use ethexe_processor::RunnerConfig;
use futures::{Stream, stream::FusedStream};
use hyper::header::HeaderValue;
use jsonrpsee::{
    RpcModule as JsonrpcModule,
    server::{PingConfig, Server, ServerHandle},
};
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::sync::{mpsc, oneshot};
use tower_http::cors::{AllowOrigin, CorsLayer};

mod apis;
mod errors;
mod utils;

#[cfg(all(test, feature = "client"))]
mod tests;

#[derive(Debug)]
pub enum RpcEvent {
    InjectedTransaction {
        transaction: RpcOrNetworkInjectedTx,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
    },
}

/// Configuration of the RPC endpoint.
#[derive(Debug, Clone)]
pub struct RpcConfig {
    /// Listen address.
    pub listen_addr: SocketAddr,
    /// CORS.
    pub cors: Option<Vec<String>>,
    /// Runner config is created with the data
    /// for processor, but with gas limit multiplier applied.
    pub runner_config: RunnerConfig,
}

pub struct RpcServer {
    config: RpcConfig,
    db: Database,
}

impl RpcServer {
    pub fn new(config: RpcConfig, db: Database) -> Self {
        Self { config, db }
    }

    pub const fn port(&self) -> u16 {
        self.config.listen_addr.port()
    }

    pub async fn run_server(self) -> Result<(ServerHandle, RpcService)> {
        let (rpc_sender, rpc_receiver) = mpsc::unbounded_channel();

        let cors_layer = self.cors_layer()?;
        let http_middleware = tower::ServiceBuilder::new().layer(cors_layer);

        let server = Server::builder()
            .set_http_middleware(http_middleware)
            // Setup WebSocket pings to detect dead connections.
            // Now it is set to default: ping_interval = 30s, inactive_limit = 40s
            .enable_ws_ping(PingConfig::default())
            .build(self.config.listen_addr)
            .await?;

        let server_apis = self.server_apis(rpc_sender);
        let injected_api = server_apis.injected.clone();

        let handle = server.start(server_apis.into_methods());

        Ok((handle, RpcService::new(rpc_receiver, injected_api)))
    }

    fn cors_layer(&self) -> Result<CorsLayer> {
        let Some(cors) = self.config.cors.clone() else {
            return Ok(CorsLayer::permissive());
        };

        let mut list = Vec::new();
        for origin in cors {
            list.push(HeaderValue::from_str(&origin)?)
        }

        Ok(CorsLayer::new().allow_origin(AllowOrigin::list(list)))
    }

    fn server_apis(&self, sender: mpsc::UnboundedSender<RpcEvent>) -> RpcServerApis {
        RpcServerApis {
            code: CodeApi::new(self.db.clone()),
            block: BlockApi::new(self.db.clone()),
            program: ProgramApi::new(self.db.clone(), self.config.runner_config.clone()),
            injected: InjectedApi::new(sender),
        }
    }
}

pub struct RpcService {
    receiver: mpsc::UnboundedReceiver<RpcEvent>,
    injected_api: InjectedApi,
}

impl RpcService {
    pub fn new(receiver: mpsc::UnboundedReceiver<RpcEvent>, injected_api: InjectedApi) -> Self {
        Self {
            receiver,
            injected_api,
        }
    }

    pub fn provide_promises_batch(&self, promises: Vec<SignedPromise>) {
        let injected_api = self.injected_api.clone();

        promises.into_iter().for_each(|promise| {
            injected_api.send_promise(promise);
        });
    }
}

impl Stream for RpcService {
    type Item = RpcEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

impl FusedStream for RpcService {
    fn is_terminated(&self) -> bool {
        self.receiver.is_closed()
    }
}

struct RpcServerApis {
    pub block: BlockApi,
    pub code: CodeApi,
    pub injected: InjectedApi,
    pub program: ProgramApi,
}

impl RpcServerApis {
    pub fn into_methods(self) -> jsonrpsee::server::RpcModule<()> {
        let mut module = JsonrpcModule::new(());

        module
            .merge(BlockServer::into_rpc(self.block))
            .expect("No conflicts");
        module
            .merge(CodeServer::into_rpc(self.code))
            .expect("No conflicts");
        module
            .merge(InjectedServer::into_rpc(self.injected))
            .expect("No conflicts");
        module
            .merge(ProgramServer::into_rpc(self.program))
            .expect("No conflicts");

        module
    }
}
