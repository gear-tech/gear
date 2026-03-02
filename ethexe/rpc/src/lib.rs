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

pub use crate::apis::SnapshotStreamItem;
#[cfg(feature = "client")]
pub use crate::apis::{
    BlockClient, CodeClient, FullProgramState, InjectedClient, ProgramClient, SnapshotClient,
};

use anyhow::Result;
use apis::{
    BlockApi, BlockServer, CodeApi, CodeServer, InjectedApi, InjectedServer, ProgramApi,
    ProgramServer, SnapshotApi, SnapshotServer,
};
use ethexe_common::injected::{
    AddressedInjectedTransaction, InjectedTransactionAcceptance, SignedPromise,
};
use ethexe_db::{Database, RocksDatabase};
use ethexe_processor::{Processor, ProcessorConfig};
use futures::{FutureExt, Stream, future::BoxFuture, stream::FusedStream};
use hyper::header::{AUTHORIZATION, HeaderValue};
use jsonrpsee::{
    RpcModule as JsonrpcModule,
    core::server::MethodResponse,
    server::{
        PingConfig, Server, ServerHandle,
        middleware::rpc::{RpcServiceBuilder, RpcServiceT},
    },
    types::Request,
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
mod metrics;
mod utils;

#[cfg(all(test, feature = "client"))]
mod tests;

pub const DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER: u64 = 10;

#[derive(Debug)]
pub enum RpcEvent {
    InjectedTransaction {
        transaction: AddressedInjectedTransaction,
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
    /// Gas allowance for each reply calculation.
    pub gas_allowance: u64,
    /// Amount of processing threads for queue processing.
    pub chunk_size: usize,
    /// Configuration for snapshot RPC API.
    pub snapshot: Option<SnapshotRpcConfig>,
}

/// Configuration of RocksDB snapshot download RPC.
#[derive(Debug, Clone)]
pub struct SnapshotRpcConfig {
    /// Static bearer token used to authorize snapshot methods.
    pub auth_bearer_token: String,
    /// Size of one streamed chunk in bytes.
    pub chunk_size_bytes: usize,
    /// Snapshot retention period in seconds.
    pub retention_secs: u64,
    /// Max number of concurrent snapshot downloads.
    pub max_concurrent_downloads: u32,
}

impl SnapshotRpcConfig {
    pub const DEFAULT_CHUNK_SIZE_BYTES: usize = 1024 * 1024;
    pub const DEFAULT_RETENTION_SECS: u64 = 600;
    pub const DEFAULT_MAX_CONCURRENT_DOWNLOADS: u32 = 1;
}

pub struct RpcServer {
    config: RpcConfig,
    db: Database,
    snapshot_db: Option<RocksDatabase>,
}

impl RpcServer {
    pub fn new(config: RpcConfig, db: Database) -> Self {
        Self {
            config,
            db,
            snapshot_db: None,
        }
    }

    pub fn with_snapshot_source(mut self, snapshot_db: RocksDatabase) -> Self {
        self.snapshot_db = Some(snapshot_db);
        self
    }

    pub const fn port(&self) -> u16 {
        self.config.listen_addr.port()
    }

    pub async fn run_server(self) -> Result<(ServerHandle, RpcService)> {
        let (rpc_sender, rpc_receiver) = mpsc::unbounded_channel();

        let cors_layer = self.cors_layer()?;
        let http_middleware = tower::ServiceBuilder::new().layer(cors_layer).map_request(
            |mut req: jsonrpsee::server::HttpRequest<_>| {
                let token = parse_bearer_token(
                    req.headers()
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok()),
                );
                req.extensions_mut().insert(RpcBearerToken(token));
                req
            },
        );
        let expected_bearer_token = self
            .config
            .snapshot
            .as_ref()
            .map(|cfg| cfg.auth_bearer_token.clone());
        let rpc_middleware =
            RpcServiceBuilder::new().layer_fn(move |service| SnapshotAuthRpcMiddleware {
                service,
                expected_bearer_token: expected_bearer_token.clone(),
            });

        let server = Server::builder()
            .set_http_middleware(http_middleware)
            .set_rpc_middleware(rpc_middleware)
            // Setup WebSocket pings to detect dead connections.
            // Now it is set to default: ping_interval = 30s, inactive_limit = 40s
            .enable_ws_ping(PingConfig::default())
            .build(self.config.listen_addr)
            .await?;

        let processor = Processor::with_config(
            ProcessorConfig {
                chunk_size: self.config.chunk_size,
            },
            self.db.clone(),
        )?;

        let snapshot = if let Some(snapshot_config) = self.config.snapshot.clone() {
            self.snapshot_db
                .clone()
                .map(|snapshot_db| SnapshotApi::new(self.db.clone(), snapshot_db, snapshot_config))
        } else {
            None
        };
        if self.config.snapshot.is_some() && snapshot.is_none() {
            tracing::warn!(
                "snapshot rpc is configured, but no rocksdb source was provided; snapshot methods are disabled"
            );
        }

        let server_apis = RpcServerApis {
            code: CodeApi::new(self.db.clone()),
            block: BlockApi::new(self.db.clone()),
            program: ProgramApi::new(self.db.clone(), processor, self.config.gas_allowance),
            injected: InjectedApi::new(rpc_sender),
            snapshot,
        };
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
}

pub struct RpcService {
    /// Receiver for incoming RPC events to forward to the main service.
    receiver: mpsc::UnboundedReceiver<RpcEvent>,
    /// Injected API implementation.
    injected_api: InjectedApi,
}

impl RpcService {
    pub fn new(receiver: mpsc::UnboundedReceiver<RpcEvent>, injected_api: InjectedApi) -> Self {
        Self {
            receiver,
            injected_api,
        }
    }

    /// Provides a promise inside RPC service to be sent to subscribers.
    pub fn provide_promise(&self, promise: SignedPromise) {
        self.injected_api.send_promise(promise);
    }

    /// Provides a bundle of promises inside RPC service to be sent to subscribers.
    pub fn provide_promises(&self, promises: Vec<SignedPromise>) {
        promises.into_iter().for_each(|promise| {
            self.provide_promise(promise);
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
    pub snapshot: Option<SnapshotApi>,
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
        if let Some(snapshot) = self.snapshot {
            module
                .merge(SnapshotServer::into_rpc(snapshot))
                .expect("No conflicts");
        }

        module
    }
}

#[derive(Clone, Debug)]
struct RpcBearerToken(Option<String>);

#[derive(Clone, Debug)]
struct SnapshotAuthRpcMiddleware<S> {
    service: S,
    expected_bearer_token: Option<String>,
}

impl<'a, S> RpcServiceT<'a> for SnapshotAuthRpcMiddleware<S>
where
    S: RpcServiceT<'a> + 'a,
    S::Future: Send + 'a,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, request: Request<'a>) -> Self::Future {
        if request.method_name().starts_with("snapshot_")
            && self.expected_bearer_token.is_some()
            && !self.is_authorized(&request)
        {
            let id = request.id().into_owned();
            return async move {
                MethodResponse::error(id, errors::unauthorized("invalid or missing bearer token"))
            }
            .boxed();
        }

        self.service.call(request).boxed()
    }
}

impl<S> SnapshotAuthRpcMiddleware<S> {
    fn is_authorized(&self, request: &Request<'_>) -> bool {
        let expected = self.expected_bearer_token.as_deref();
        let actual = request
            .extensions()
            .get::<RpcBearerToken>()
            .and_then(|token| token.0.as_deref());
        actual == expected
    }
}

fn parse_bearer_token(header: Option<&str>) -> Option<String> {
    let header = header?;
    let (scheme, token) = header.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
        return None;
    }

    Some(token.to_owned())
}
