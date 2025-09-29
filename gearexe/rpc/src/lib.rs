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

use anyhow::{Result, anyhow};
use apis::{
    BlockApi, BlockServer, CodeApi, CodeServer, DevApi, DevServer, ProgramApi, ProgramServer,
    TransactionPoolApi, TransactionPoolServer,
};
use gearexe_blob_loader::local::LocalBlobStorage;
use gearexe_common::tx_pool::SignedOffchainTransaction;
use gearexe_db::Database;
use futures::{FutureExt, Stream, stream::FusedStream};
use gprimitives::H256;
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

mod apis;
mod common;
mod errors;

#[cfg(feature = "test-utils")]
pub mod test_utils;
pub(crate) mod util;

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
    /// Dev mode.
    pub dev: bool,
}

pub struct RpcService {
    config: RpcConfig,
    db: Database,
    blobs_storage: Option<LocalBlobStorage>,
}

impl RpcService {
    pub fn new(config: RpcConfig, db: Database, blobs_storage: Option<LocalBlobStorage>) -> Self {
        Self {
            config,
            db,
            blobs_storage,
        }
    }

    pub const fn port(&self) -> u16 {
        self.config.listen_addr.port()
    }

    pub async fn run_server(self) -> Result<(ServerHandle, RpcReceiver)> {
        let (rpc_sender, rpc_receiver) = mpsc::unbounded_channel();

        let listener = TcpListener::bind(self.config.listen_addr).await?;

        let cors = util::try_into_cors(self.config.cors)?;

        let http_middleware = tower::ServiceBuilder::new().layer(cors);

        let service_builder = Server::builder()
            .set_http_middleware(http_middleware)
            .to_service_builder();

        let mut module = JsonrpcModule::new(());
        module.merge(ProgramServer::into_rpc(ProgramApi::new(self.db.clone())))?;
        module.merge(BlockServer::into_rpc(BlockApi::new(self.db.clone())))?;
        module.merge(CodeServer::into_rpc(CodeApi::new(self.db.clone())))?;
        module.merge(TransactionPoolServer::into_rpc(TransactionPoolApi::new(
            rpc_sender,
        )))?;

        if self.config.dev {
            module.merge(DevServer::into_rpc(DevApi::new(
                self.blobs_storage.unwrap().clone(),
            )))?;
        }

        let (stop_handle, server_handle) = stop_channel();

        let cfg = PerConnection {
            methods: module.into(),
            stop_handle: stop_handle.clone(),
            svc_builder: service_builder,
        };

        tokio::spawn(async move {
            loop {
                let socket = tokio::select! {
                    res = listener.accept() => {
                        match res {
                            Ok((socket, _)) => socket,
                            Err(e) => {
                                log::error!("Failed to accept connection: {e:?}");
                                continue;
                            }
                        }
                    }
                    _ = cfg.stop_handle.clone().shutdown() => {
                        log::info!("Shutdown signal received, stopping server.");
                        break;
                    }
                };

                let cfg2 = cfg.clone();

                let svc = tower::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                    let PerConnection {
                        methods,
                        stop_handle,
                        svc_builder,
                    } = cfg2.clone();

                    let is_ws = jsonrpsee::server::ws::is_upgrade_request(&req);

                    let mut svc = svc_builder.build(methods, stop_handle);

                    if is_ws {
                        let session_close = svc.on_session_closed();

                        tokio::spawn(async move {
                            session_close.await;
                            log::info!("WebSocket connection closed");
                        });

                        async move {
                            log::info!("WebSocket connection accepted");

                            svc.call(req).await.map_err(|e| anyhow!("Error: {:?}", e))
                        }
                        .boxed()
                    } else {
                        async move { svc.call(req).await.map_err(|e| anyhow!("Error: {:?}", e)) }
                            .boxed()
                    }
                });

                tokio::spawn(serve_with_graceful_shutdown(
                    socket,
                    svc,
                    stop_handle.clone().shutdown(),
                ));
            }
        });

        Ok((server_handle, RpcReceiver(rpc_receiver)))
    }
}

pub struct RpcReceiver(mpsc::UnboundedReceiver<RpcEvent>);

impl Stream for RpcReceiver {
    type Item = RpcEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

impl FusedStream for RpcReceiver {
    fn is_terminated(&self) -> bool {
        self.0.is_closed()
    }
}

#[derive(Debug)]
pub enum RpcEvent {
    OffchainTransaction {
        transaction: SignedOffchainTransaction,
        response_sender: Option<oneshot::Sender<Result<H256>>>,
    },
}
