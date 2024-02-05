// This file is part of Gear.
//
// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! RPC client for Gear API.
use crate::{
    config::GearConfig,
    result::{Error, Result},
};
use futures_util::{StreamExt, TryStreamExt};
use jsonrpsee::{
    core::{
        client::{ClientT, Subscription, SubscriptionClientT, SubscriptionKind},
        traits::ToRpcParams,
        Error as JsonRpseeError,
    },
    http_client::{HttpClient, HttpClientBuilder},
    types::SubscriptionId,
    ws_client::{WsClient, WsClientBuilder},
};
use sp_runtime::DeserializeOwned;
use std::{ops::Deref, result::Result as StdResult, time::Duration};
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::{
            RawRpcFuture as RpcFuture, RawRpcSubscription as RpcSubscription, RawValue,
            RpcClient as SubxtRpcClient, RpcClientT, RpcParams,
        },
    },
    error::RpcError,
};

const DEFAULT_GEAR_ENDPOINT: &str = "wss://rpc.vara.network:443";
const DEFAULT_TIMEOUT: u64 = 60_000;
const ONE_HUNDRED_MEGA_BYTES: u32 = 100 * 1024 * 1024;

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

        log::info!("Connecting to {url} ...");
        if url.starts_with("ws") {
            Ok(Self::Ws(
                WsClientBuilder::default()
                    // Actually that stand for the response too.
                    // *WARNING*:
                    // After updating jsonrpsee to 0.20.0 and higher
                    // use another method created only for that.
                    .max_request_body_size(ONE_HUNDRED_MEGA_BYTES)
                    .connection_timeout(Duration::from_millis(timeout))
                    .request_timeout(Duration::from_millis(timeout))
                    .build(url)
                    .await
                    .map_err(Error::SubxtRpc)?,
            ))
        } else if url.starts_with("http") {
            Ok(Self::Http(
                HttpClientBuilder::default()
                    .request_timeout(Duration::from_millis(timeout))
                    .build(url)
                    .map_err(Error::SubxtRpc)?,
            ))
        } else {
            Err(Error::InvalidUrl)
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
            let stream = match self {
                RpcClient::Http(c) => subscription_stream(c, sub, params, unsub).await?,
                RpcClient::Ws(c) => subscription_stream(c, sub, params, unsub).await?,
            };

            let id = match stream.kind() {
                SubscriptionKind::Subscription(SubscriptionId::Str(id)) => {
                    Some(id.clone().into_owned())
                }
                _ => None,
            };

            let stream = stream
                .map_err(|e| RpcError::ClientError(Box::new(e)))
                .boxed();

            Ok(RpcSubscription { stream, id })
        })
    }
}

// This is a support fn to create a subscription stream for a generic client
async fn subscription_stream<C: SubscriptionClientT>(
    client: &C,
    sub: &str,
    params: Option<Box<RawValue>>,
    unsub: &str,
) -> StdResult<Subscription<Box<RawValue>>, RpcError> {
    SubscriptionClientT::subscribe::<Box<RawValue>, _>(client, sub, Params(params), unsub)
        .await
        .map_err(|e| RpcError::ClientError(Box::new(e)))
}

/// RPC client for Gear API.
#[derive(Clone)]
pub struct Rpc {
    rpc: SubxtRpcClient,
    methods: LegacyRpcMethods<GearConfig>,
}

impl Rpc {
    /// Create RPC client from url and timeout.
    pub async fn new(url: Option<&str>, timeout: Option<u64>) -> Result<Self> {
        let rpc = SubxtRpcClient::new(RpcClient::new(url, timeout).await?);
        let methods = LegacyRpcMethods::new(rpc.clone());
        Ok(Self { rpc, methods })
    }

    /// Get RPC client.
    pub fn client(&self) -> SubxtRpcClient {
        self.rpc.clone()
    }

    /// Raw RPC request
    pub async fn request<Res: DeserializeOwned>(
        &self,
        method: &str,
        params: RpcParams,
    ) -> Result<Res> {
        self.rpc.request(method, params).await.map_err(Into::into)
    }
}

impl Deref for Rpc {
    type Target = LegacyRpcMethods<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.methods
    }
}
