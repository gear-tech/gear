// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # ethexe-rpc
//!
//! JSON-RPC 2.0 server and client APIs for an ethexe (Vara.eth) node. Exposes Ethereum
//! block data, WASM code blobs, and program state to external callers, and bridges
//! injected-transaction submission back to the main service. It reads `ethexe-db` and
//! writes only injected-transaction promise/receipt caches; its sole outbound action is
//! emitting [`RpcEvent::InjectedTransaction`] for the main service to handle.
//!
//! ## Role in the Stack
//!
//! `ethexe-service` constructs an [`RpcServer`], calls [`RpcServer::run_server`], then polls
//! the returned [`RpcService`] stream for [`RpcEvent`]s and feeds results back via
//! [`RpcService::receive_computed_promise`] and [`RpcService::receive_tx_receipt`]. The typed
//! clients (behind the `client` feature) are consumed by `ethexe-cli`, `ethexe-sdk`, and test
//! tooling.
//!
//! ## Public API
//!
//! | Item | Feature | Description |
//! |------|---------|-------------|
//! | [`RpcServer`] | `server` | Owns config + DB; `new(config, db)` then `run_server()` starts the endpoint. |
//! | [`RpcConfig`] | `server` | Listen address, CORS, gas allowance, program-processing chunk size, dev-API flag. |
//! | [`RpcService`] | `server` | `Stream<Item = RpcEvent>` the main service polls; accepts promise/receipt results from the service and forwards them to JSON-RPC subscribers. |
//! | [`RpcEvent`] | `server` | Outbound work items; currently only `InjectedTransaction`. |
//! | [`DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER`] | always | Default gas-limit multiplier (10). |
//! | [`BlockClient`], [`CodeClient`], [`ProgramClient`], [`InjectedClient`], [`DevClient`] | `client` | Generated typed clients. |
//! | [`FullProgramState`], [`CalculateReplyForHandleResult`] | `client` | Result types re-exported for client callers. |

#[cfg(feature = "client")]
pub use crate::apis::{
    BlockClient, CalculateReplyForHandleResult, CodeClient, DevClient, FullProgramState,
    InjectedClient, ProgramClient,
};

#[cfg(feature = "server")]
use anyhow::Result;
#[cfg(feature = "server")]
use apis::{
    BlockApi, BlockServer, CodeApi, CodeServer, DevApi, DevServer, InjectedApi, InjectedServer,
    ProgramApi, ProgramServer,
};
#[cfg(feature = "server")]
use ethexe_common::injected::{
    AddressedInjectedTransaction, InjectedTransactionAcceptance, Promise, SignedCompactTxReceipt,
};
#[cfg(feature = "server")]
use ethexe_db::Database;
#[cfg(feature = "server")]
use ethexe_processor::{Processor, ProcessorConfig};
#[cfg(feature = "server")]
use futures::{Stream, stream::FusedStream};
#[cfg(feature = "server")]
use hyper::header::HeaderValue;
#[cfg(feature = "server")]
use jsonrpsee::{
    RpcModule as JsonrpcModule,
    server::{PingConfig, RpcServiceBuilder, Server, ServerHandle},
};
#[cfg(feature = "server")]
use metrics::RpcMetricsLayer;
#[cfg(feature = "server")]
use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};
#[cfg(feature = "server")]
use tokio::sync::{mpsc, oneshot};
#[cfg(feature = "server")]
use tower_http::cors::{AllowOrigin, CorsLayer};

mod apis;
#[cfg(feature = "server")]
mod errors;
#[cfg(feature = "server")]
mod metrics;
#[cfg(feature = "server")]
mod utils;

#[cfg(all(test, feature = "client"))]
mod tests;

pub const DEFAULT_BLOCK_GAS_LIMIT_MULTIPLIER: u64 = 10;

#[cfg(feature = "server")]
#[derive(Debug)]
pub enum RpcEvent {
    InjectedTransaction {
        transaction: AddressedInjectedTransaction,
        response_sender: oneshot::Sender<InjectedTransactionAcceptance>,
    },
}

/// Configuration of the RPC endpoint.
#[cfg(feature = "server")]
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
    /// Flag to enable RPC DevApi.
    /// Important: can be enabled only in dev mode, run: `ethexe run --dev`
    pub with_dev_api: bool,
}

#[cfg(feature = "server")]
pub struct RpcServer {
    config: RpcConfig,
    db: Database,
}

#[cfg(feature = "server")]
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
        // Setup the default RPC metrics layer.
        let rpc_middleware = RpcServiceBuilder::new().layer(RpcMetricsLayer);

        let processor = Processor::with_config(
            ProcessorConfig {
                chunk_size: self.config.chunk_size,
            },
            self.db.clone(),
        )?
        .overlaid();

        let server_apis = RpcServerApis {
            code: CodeApi::new(self.db.clone()),
            block: BlockApi::new(self.db.clone()),
            program: ProgramApi::new(self.db.clone(), processor, self.config.gas_allowance),
            injected: InjectedApi::new(self.db.clone(), rpc_sender),
            dev: self
                .config
                .with_dev_api
                .then(|| DevApi::new(self.db.clone())),
        };
        let injected_api = server_apis.injected.clone();

        let server_handle = Server::builder()
            .set_http_middleware(http_middleware)
            .set_rpc_middleware(rpc_middleware)
            // Setup WebSocket pings to detect dead connections.
            // Now it is set to default: ping_interval = 30s, inactive_limit = 40s
            .enable_ws_ping(PingConfig::default())
            .build(self.config.listen_addr)
            .await?
            .start(server_apis.into_module());

        Ok((server_handle, RpcService::new(rpc_receiver, injected_api)))
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

#[cfg(feature = "server")]
pub struct RpcService {
    /// Receiver for incoming RPC events to forward to the main service.
    receiver: mpsc::UnboundedReceiver<RpcEvent>,
    /// Injected API implementation.
    injected_api: InjectedApi,
}

#[cfg(feature = "server")]
impl RpcService {
    pub fn new(receiver: mpsc::UnboundedReceiver<RpcEvent>, injected_api: InjectedApi) -> Self {
        Self {
            receiver,
            injected_api,
        }
    }

    pub fn receive_computed_promise(&self, promise: Promise) {
        self.injected_api.on_computed_promise(promise);
    }

    pub fn receive_tx_receipt(&self, receipt: SignedCompactTxReceipt) {
        self.injected_api.on_tx_receipt(receipt);
    }
}

#[cfg(feature = "server")]
impl Stream for RpcService {
    type Item = RpcEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(cx)
    }
}

#[cfg(feature = "server")]
impl FusedStream for RpcService {
    fn is_terminated(&self) -> bool {
        self.receiver.is_closed()
    }
}

#[cfg(feature = "server")]
struct RpcServerApis {
    pub block: BlockApi,
    pub code: CodeApi,
    pub injected: InjectedApi,
    pub program: ProgramApi,
    pub dev: Option<DevApi>,
}

#[cfg(feature = "server")]
impl RpcServerApis {
    pub fn into_module(self) -> jsonrpsee::server::RpcModule<()> {
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
        if let Some(dev) = self.dev {
            module
                .merge(DevServer::into_rpc(dev))
                .expect("No conflicts");
        }

        module
    }
}
