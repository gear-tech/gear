use ethexe_db::{BlockHeader, BlockMetaStorage, Database};
use ethexe_processor::Processor;
use futures::FutureExt;
use gear_core::message::ReplyInfo;
use gprimitives::{H160, H256};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
    server::{
        serve_with_graceful_shutdown, stop_channel, Server, ServerHandle, StopHandle,
        TowerServiceBuilder,
    },
    types::ErrorObject,
    Methods,
};
use sp_core::Bytes;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower::Service;

#[derive(Clone)]
struct PerConnection<RpcMiddleware, HttpMiddleware> {
    methods: Methods,
    stop_handle: StopHandle,
    svc_builder: TowerServiceBuilder<RpcMiddleware, HttpMiddleware>,
}

#[rpc(server)]
pub trait RpcApi {
    #[method(name = "blockHeader")]
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)>;

    #[method(name = "calculateReplyForHandle")]
    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo>;
}

pub struct RpcModule {
    db: Database,
}

impl RpcModule {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub fn block_header_at_or_latest(
        &self,
        at: impl Into<Option<H256>>,
    ) -> RpcResult<(H256, BlockHeader)> {
        if let Some(hash) = at.into() {
            self.db
                .block_header(hash)
                .map(|header| (hash, header))
                .ok_or_else(|| db_err("Block header for requested hash wasn't found"))
        } else {
            self.db
                .latest_valid_block()
                .ok_or_else(|| db_err("Latest block header wasn't found"))
        }
    }
}

#[async_trait]
impl RpcApiServer for RpcModule {
    async fn block_header(&self, hash: Option<H256>) -> RpcResult<(H256, BlockHeader)> {
        self.block_header_at_or_latest(hash)
    }

    async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> RpcResult<ReplyInfo> {
        let block_hash = self.block_header_at_or_latest(at)?.0;

        // TODO (breathx): spawn in a new thread and catch panics. (?) Generally catch runtime panics (?).
        // TODO (breathx): optimize here instantiation if matches actual runtime.
        let processor = Processor::new(self.db.clone()).map_err(|_| internal())?;

        let mut overlaid_processor = processor.overlaid();

        overlaid_processor
            .execute_for_reply(
                block_hash,
                source.into(),
                program_id.into(),
                payload.0,
                value,
            )
            .map_err(runtime_err)
    }
}

fn db_err(err: &'static str) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Database error", Some(err))
}

fn runtime_err(err: anyhow::Error) -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Runtime error", Some(format!("{err}")))
}

fn internal() -> ErrorObject<'static> {
    ErrorObject::owned(8000, "Internal error", None::<&str>)
}

pub struct RpcConfig {
    port: u16,
    db: Database,
}

pub struct RpcService {
    config: RpcConfig,
}

impl RpcService {
    pub fn new(port: u16, db: Database) -> Self {
        Self {
            config: RpcConfig { port, db },
        }
    }

    pub async fn run_server(self) -> anyhow::Result<(ServerHandle, u16)> {
        let listener =
            TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], self.config.port))).await?;

        let service_builder = Server::builder().to_service_builder();
        let module = RpcApiServer::into_rpc(RpcModule::new(self.config.db));

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
                                log::error!("Failed to accept connection: {:?}", e);
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

                            svc.call(req)
                                .await
                                .map_err(|e| anyhow::anyhow!("Error: {:?}", e))
                        }
                        .boxed()
                    } else {
                        async move {
                            svc.call(req)
                                .await
                                .map_err(|e| anyhow::anyhow!("Error: {:?}", e))
                        }
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

        Ok((server_handle, self.config.port))
    }
}
