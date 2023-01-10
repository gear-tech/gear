//! RPC client for Gear API.
use crate::result::{ClientError, Result};
use futures_util::{StreamExt, TryStreamExt};
use jsonrpsee::{
    core::{
        client::{ClientT, SubscriptionClientT},
        traits::ToRpcParams,
        Error as JsonRpseeError,
    },
    http_client::{HttpClient, HttpClientBuilder},
    ws_client::{WsClient, WsClientBuilder},
};
use std::{result::Result as StdResult, time::Duration};
use subxt::{
    error::RpcError,
    rpc::{RawValue, RpcClientT, RpcFuture, RpcSubscription},
};

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc-node.gear-tech.io:443";
const DEFAULT_TIMEOUT: u64 = 60_000;

struct Params(Option<Box<RawValue>>);

impl ToRpcParams for Params {
    fn to_rpc_params(self) -> StdResult<Option<Box<RawValue>>, JsonRpseeError> {
        Ok(self.0)
    }
}

/// Either http or websocket RPC client
pub enum RpcClient {
    Ws(WsClient),
    Http(HttpClient),
}

impl RpcClient {
    /// Create RPC client from url and timeout.
    pub async fn new(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        let (url, timeout) = (
            url.unwrap_or(DEFAULT_GEAR_ENDPOINT),
            timeout.unwrap_or(DEFAULT_TIMEOUT),
        );

        if url.starts_with("ws") {
            Ok(Self::Ws(
                WsClientBuilder::default()
                    .connection_timeout(Duration::from_millis(timeout))
                    .request_timeout(Duration::from_millis(timeout))
                    .build(url)
                    .await
                    .map_err(ClientError::SubxtRpc)?,
            ))
        } else if url.starts_with("http") {
            Ok(Self::Http(
                HttpClientBuilder::default()
                    .request_timeout(Duration::from_millis(timeout))
                    .build(url)
                    .map_err(ClientError::SubxtRpc)?,
            ))
        } else {
            Err(ClientError::InvalidUrl.into())
        }
    }
}

impl RpcClientT for RpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<RawValue>>,
    ) -> RpcFuture<'a, Box<RawValue>> {
        Box::pin(async move {
            let res = match self {
                RpcClient::Http(c) => ClientT::request(c, method, Params(params))
                    .await
                    .map_err(|e| RpcError::ClientError(Box::new(e)))?,
                RpcClient::Ws(c) => ClientT::request(c, method, Params(params))
                    .await
                    .map_err(|e| RpcError::ClientError(Box::new(e)))?,
            };
            Ok(res)
        })
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<RawValue>>,
        unsub: &'a str,
    ) -> RpcFuture<'a, RpcSubscription> {
        Box::pin(async move {
            let sub = match self {
                RpcClient::Http(c) => SubscriptionClientT::subscribe::<Box<RawValue>, _>(
                    c,
                    sub,
                    Params(params),
                    unsub,
                )
                .await
                .map_err(|e| RpcError::ClientError(Box::new(e)))?
                .map_err(|e| RpcError::ClientError(Box::new(e)))
                .boxed(),
                RpcClient::Ws(c) => SubscriptionClientT::subscribe::<Box<RawValue>, _>(
                    c,
                    sub,
                    Params(params),
                    unsub,
                )
                .await
                .map_err(|e| RpcError::ClientError(Box::new(e)))?
                .map_err(|e| RpcError::ClientError(Box::new(e)))
                .boxed(),
            };

            Ok(sub)
        })
    }
}
