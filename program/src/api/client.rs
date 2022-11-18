//! RPC client for Gear API.
use crate::result::{ClientError, Result};
use futures_util::{StreamExt, TryStreamExt};
use jsonrpsee::{
    core::client::{ClientT, SubscriptionClientT},
    http_client::{HttpClient, HttpClientBuilder},
    types::ParamsSer,
    ws_client::{WsClient, WsClientBuilder},
};
use serde_json::value::Value;
use std::{result::Result as StdResult, time::Duration};
use subxt::{
    error::RpcError,
    rpc::{RawValue, RpcClientT, RpcFuture, RpcSubscription},
};

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc-node.gear-tech.io:443";
const DEFAULT_TIMEOUT: u64 = 60_000;

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
            let params = prep_params_for_jsonrpsee(params)?;
            let res = match self {
                RpcClient::Http(c) => ClientT::request(c, method, Some(params))
                    .await
                    .map_err(|e| RpcError(e.to_string()))?,
                RpcClient::Ws(c) => ClientT::request(c, method, Some(params))
                    .await
                    .map_err(|e| RpcError(e.to_string()))?,
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
            let params = prep_params_for_jsonrpsee(params)?;
            let sub = match self {
                RpcClient::Http(c) => {
                    SubscriptionClientT::subscribe::<Box<RawValue>>(c, sub, Some(params), unsub)
                        .await
                        .map_err(|e| RpcError(e.to_string()))?
                        .map_err(|e| RpcError(e.to_string()))
                        .boxed()
                }
                RpcClient::Ws(c) => {
                    SubscriptionClientT::subscribe::<Box<RawValue>>(c, sub, Some(params), unsub)
                        .await
                        .map_err(|e| RpcError(e.to_string()))?
                        .map_err(|e| RpcError(e.to_string()))
                        .boxed()
                }
            };

            Ok(sub)
        })
    }
}

// This is ugly; we have to encode to Value's to be compat with the jsonrpc interface.
// Remove and simplify this once something like https://github.com/paritytech/jsonrpsee/issues/862 is in:
fn prep_params_for_jsonrpsee(
    params: Option<Box<RawValue>>,
) -> StdResult<ParamsSer<'static>, RpcError> {
    let params = match params {
        Some(params) => params,
        // No params? avoid any work and bail early.
        None => return Ok(ParamsSer::Array(Vec::new())),
    };
    let val = serde_json::to_value(&params).expect("RawValue guarantees valid JSON");
    let arr = match val {
        Value::Array(arr) => Ok(arr),
        _ => Err(RpcError(format!(
            "RPC Params are expected to be an array but got {params}"
        ))),
    }?;
    Ok(ParamsSer::Array(arr))
}
