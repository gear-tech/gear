use ethexe_db::{BlockHeader, BlockMetaStorage, Database};
use futures::FutureExt;
use gprimitives::H256;
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
    server::{
        serve_with_graceful_shutdown, stop_channel, Server, ServerHandle, StopHandle,
        TowerServiceBuilder,
    },
    types::{ErrorCode, ErrorObject},
    Methods,
};
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
    async fn block_header(&self, hash: H256) -> RpcResult<BlockHeader>;
}

pub struct RpcModule {
    db: Database,
}

impl RpcModule {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

#[async_trait]
impl RpcApiServer for RpcModule {
    async fn block_header(&self, hash: H256) -> RpcResult<BlockHeader> {
        // let db = db.lock().await;
        self.db.block_header(hash).ok_or_else(|| {
            ErrorObject::borrowed(ErrorCode::InvalidParams.code(), "Block not found", None)
        })
    }
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
