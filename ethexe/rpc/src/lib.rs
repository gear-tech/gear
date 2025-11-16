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

#[cfg(feature = "test-utils")]
pub use crate::apis::InjectedClient;

use anyhow::{Result, anyhow};
use apis::{
    BlockApi, BlockServer, CodeApi, CodeServer, InjectedApi, InjectedServer, ProgramApi,
    ProgramServer,
};
use ethexe_common::injected::{RpcOrNetworkInjectedTx, SignedPromise};
use ethexe_db::Database;
use ethexe_processor::RunnerConfig;
use futures::{FutureExt, Stream, stream::FusedStream};
use hyper::header::HeaderValue;
use jsonrpsee::{
    Methods, RpcModule as JsonrpcModule,
    server::{
        Server, ServerHandle, StopHandle, TowerServiceBuilder, serve_with_graceful_shutdown,
        stop_channel,
    },
};
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::{
    net::TcpListener,
    sync::{mpsc, oneshot},
};
use tower::Service;
use tower_http::cors::{AllowOrigin, CorsLayer};

mod apis;
mod errors;
mod utils;

#[cfg(feature = "test-utils")]
pub mod test_utils;

#[derive(Debug)]
pub enum RpcEvent {
    InjectedTransaction {
        transaction: RpcOrNetworkInjectedTx,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
    },
    InjectedTransactionSubscription {
        transaction: RpcOrNetworkInjectedTx,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
        promise_sender: oneshot::Sender<SignedPromise>,
    },
}

#[derive(Debug)]
pub(crate) enum RpcEventInner {
    InjectedTransaction {
        transaction: RpcOrNetworkInjectedTx,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
    },
    PromiseSubscription {
        transaction: RpcOrNetworkInjectedTx,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
        promise_sender: oneshot::Sender<SignedPromise>,
    },
}

#[derive(Clone)]
struct PerConnection<RpcMiddleware, HttpMiddleware> {
    methods: Methods,
    stop_handle: StopHandle,
    svc_builder: TowerServiceBuilder<RpcMiddleware, HttpMiddleware>,
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

pub struct RpcService {
    config: RpcConfig,
    db: Database,
}

impl RpcService {
    pub fn new(config: RpcConfig, db: Database) -> Self {
        Self { config, db }
    }

    pub const fn port(&self) -> u16 {
        self.config.listen_addr.port()
    }

    pub async fn run_server(self) -> Result<(ServerHandle, RpcReceiver)> {
        let (rpc_sender, rpc_receiver) = mpsc::unbounded_channel();

        let cors_layer = self.cors_layer()?;
        let http_middleware = tower::ServiceBuilder::new().layer(cors_layer);

        let server = jsonrpsee::server::Server::builder()
            .set_http_middleware(http_middleware)
            .build(self.config.listen_addr)
            .await?;

        let server_apis = self.server_apis(rpc_sender);
        let injected_api = server_apis.injected.clone();

        let handle = server.start(server_apis.into_methods());

        Ok((
            handle,
            RpcReceiver {
                inner_receiver: rpc_receiver,
            },
        ))
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

pub struct RpcReceiver {
    // Event receiver from inner apis.
    inner_receiver: mpsc::UnboundedReceiver<RpcEvent>,
    //
    // outer_receiver: mpsc::UnboundedReceiver<RpcEvent>,
}

impl Stream for RpcReceiver {
    type Item = RpcEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner_receiver.poll_recv(cx)
    }
}

impl FusedStream for RpcReceiver {
    fn is_terminated(&self) -> bool {
        self.inner_receiver.is_closed()
    }
}

struct RpcServerApis {
    pub code: CodeApi,
    pub block: BlockApi,
    pub program: ProgramApi,
    pub injected: InjectedApi,
}

impl RpcServerApis {
    pub fn into_methods(self) -> jsonrpsee::server::RpcModule<()> {
        let mut module = JsonrpcModule::new(());

        module
            .merge(CodeServer::into_rpc(self.code))
            .expect("No conflicts");
        module
            .merge(BlockServer::into_rpc(self.block))
            .expect("No conflicts");
        module
            .merge(ProgramServer::into_rpc(self.program))
            .expect("No conflicts");
        module
            .merge(InjectedServer::into_rpc(self.injected))
            .expect("No conflicts");

        module
    }
}
