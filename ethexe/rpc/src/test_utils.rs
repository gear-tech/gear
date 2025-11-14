// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

//! Utilities for tests.

use crate::{InjectedTransactionAcceptance, apis::InjectedClient};
use anyhow::{Result as AnyhowResult, bail};
use ethexe_common::injected::RpcOrNetworkInjectedTx;
use jsonrpsee::{http_client::HttpClient, types::ErrorObjectOwned};
use reqwest::{Response, Result};
use serde::{Deserialize, de::DeserializeOwned};

/// Client for the ethexe rpc server.
pub struct RpcClient {
    http_client: HttpClient,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        let http_client = HttpClient::builder().build(url).unwrap();
        Self { http_client }
    }

    /// Send message using transaction pool API (`injected_sendTransaction`) of the ethexe rpc server.
    pub async fn send_injected_tx(
        &self,
        tx: RpcOrNetworkInjectedTx,
    ) -> AnyhowResult<InjectedTransactionAcceptance> {
        self.http_client
            .send_transaction(tx)
            .await
            .map_err(Into::into)
    }
}

/// Response from the ethexe rpc server.
///
/// It's a wrapper around `serde_json::Value` to provide a convenient way to extract
/// the `result` and `error` fields from the response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    inner: serde_json::Value,
}

impl JsonRpcResponse {
    /// Create a new `JsonRpcResponse` from a `Response`.
    pub async fn new(response: Response) -> Result<Self> {
        let inner = response.json().await?;

        Ok(Self { inner })
    }

    /// Try extract `result` field from the response.
    pub fn try_extract_res<T: DeserializeOwned>(&self) -> AnyhowResult<T> {
        match self.inner.get("result") {
            Some(result) => {
                let result: T = serde_json::from_value(result.clone())?;

                Ok(result)
            }
            None => bail!("No 'result' found in response"),
        }
    }

    /// Try extract `error` field from the response.
    pub fn try_extract_err(&self) -> AnyhowResult<ErrorObjectOwned> {
        match self.inner.get("error") {
            Some(error) => {
                let error = serde_json::from_value(error.clone())?;

                Ok(error)
            }
            None => bail!("No 'error' found in response"),
        }
    }
}
