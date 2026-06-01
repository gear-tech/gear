// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Vara.eth RPC client and server APIs.
//!
//! The crate provide both client and server APIs for the Vara.eth node.
//!
//! ## Crate modules
//! The crate has the following structure:
//! - `apis` - provides the RPC available APIs
//!     - `block` - Ethereum blocks API
//!     - `code` - WASM codes API
//!     - `injected` - API for communication with node via [`ethexe_common::injected::InjectedTransaction`]
//!     - `program` - WASM programs API (state, queue, mailbox, reply calculations)
//!     - `dev` - the development API (available only in development builds)
//! - `errors` - provides helpers function for building the [`jsonrpsee::types::ErrorObject`]
//! - `metrics` - provides metrics for the RPC server
//!
//! ## Design notes
//! By design the RPC server has no write access to the node database. So it must be just
//! a read-only proxy for the external users.
//!
//! ## Features
//! The following features are available:
//! - `client` - enables the client APIs generate from [`jsonrpsee::proc_macros::rpc`] macro.
//! - `server` - enables the RPC server implementation.
//!
//! ### RPC server configuration details
//! The RPC server is configured from [`RpcConfig`]. It provides the following configuration:
//! - [`RpcConfig::listen_addr`] - the address of RPC server running on
//! - [`RpcConfig::cors`] - the list of allowed CORS origins
//! - [`RpcConfig::gas_allowance`] - the gas allowance for program reply calculation
//! - [`RpcConfig::chunk_size`] - the amount of queue processing threads in message reply calculation.
//! - [`RpcConfig::with_dev_api`] - flag to enable the development API (available only in development builds)

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
