// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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
    },
    http_client::{HttpClient, HttpClientBuilder},
    types::SubscriptionId,
    ws_client::{PingConfig, WsClient, WsClientBuilder},
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

pub const ONE_HUNDRED_MEGA_BYTES: u32 = 100 * 1024 * 1024;

struct Params(Option<Box<RawValue>>);

impl ToRpcParams for Params {
    fn to_rpc_params(self) -> StdResult<Option<Box<RawValue>>, serde_json::Error> {
        Ok(self.0)
    }
}

/// Either http or websocket RPC client
#[allow(clippy::large_enum_variant)]
pub enum RpcClient {
    Ws(WsClient),
    Http(HttpClient),
}

impl RpcClient {
    /// Create RPC client from url and timeout.
    pub async fn new(uri: &str, timeout: u64) -> Result<Self> {
        log::info!("Connecting to {uri} ...");

        if uri.starts_with("ws") {
            Ok(Self::Ws(
                WsClientBuilder::default()
                    .max_request_size(ONE_HUNDRED_MEGA_BYTES)
                    .connection_timeout(Duration::from_millis(timeout))
                    .request_timeout(Duration::from_millis(timeout))
                    .enable_ws_ping(PingConfig::default())
                    .build(uri)
                    .await
                    .map_err(Error::SubxtRpc)?,
            ))
        } else if uri.starts_with("http") {
            Ok(Self::Http(
                HttpClientBuilder::default()
                    .request_timeout(Duration::from_millis(timeout))
                    .build(uri)
                    .map_err(Error::SubxtRpc)?,
            ))
        } else {
            Err(Error::InvalidUrl)
        }
    }

    /// Create WebSocket RPC client from url and timeout with a custom initializer
    /// for the WebSocket client builder.
    pub async fn new_ws_custom(
        uri: &str,
        init: impl FnOnce(WsClientBuilder) -> WsClientBuilder,
    ) -> Result<Self> {
        log::info!("Connecting to {uri} ...");

        if !uri.starts_with("ws") {
            return Err(Error::InvalidUrl);
        }

        let builder = init(WsClientBuilder::default());
        builder
            .build(uri)
            .await
            .map(Self::Ws)
            .map_err(Error::SubxtRpc)
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
    retries: u8,
}

impl Rpc {
    /// Create RPC client from url and timeout.
    pub async fn new(uri: &str, timeout: u64, retries: u8) -> Result<Self> {
        let rpc = SubxtRpcClient::new(RpcClient::new(uri, timeout).await?);
        let methods = LegacyRpcMethods::new(rpc.clone());
        Ok(Self {
            rpc,
            methods,
            retries,
        })
    }

    /// Create WebSocket RPC client from url and timeout with a custom initializer
    pub async fn new_ws_custom(
        uri: &str,
        timeout: u64,
        retries: u8,
        init: impl FnOnce(WsClientBuilder) -> WsClientBuilder,
    ) -> Result<Self> {
        let rpc = SubxtRpcClient::new(RpcClient::new_ws_custom(uri, timeout, init).await?);
        let methods = LegacyRpcMethods::new(rpc.clone());
        Ok(Self {
            rpc,
            methods,
            retries,
        })
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
        let mut retries = 0;

        loop {
            let r = self
                .rpc
                .request(method, params.clone())
                .await
                .map_err(Into::into);

            if retries == self.retries || r.is_ok() {
                return r;
            }

            retries += 1;
            log::warn!("Failed to send request: {:?}, retries: {retries}", r.err());
        }
    }
}

impl Deref for Rpc {
    type Target = LegacyRpcMethods<GearConfig>;

    fn deref(&self) -> &Self::Target {
        &self.methods
    }
}
