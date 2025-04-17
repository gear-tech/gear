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

use anyhow::{bail, Result as AnyhowResult};
use ethexe_common::tx_pool::OffchainTransaction;
use jsonrpsee::types::ErrorObjectOwned;
use reqwest::{Client, Response, Result};
use serde::{de::DeserializeOwned, Deserialize};

/// Client for the ethexe rpc server.
pub struct RpcClient {
    client: Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        let client = Client::new();

        Self { client, url }
    }

    /// Send message using transaction pool API (`transactionPool_sendMessage`) of the ethexe rpc server.
    pub async fn send_message(
        &self,
        ethexe_tx: OffchainTransaction,
        signature: Vec<u8>,
    ) -> Result<Response> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "transactionPool_sendMessage",
            "params": {
                "ethexe_tx": ethexe_tx,
                "signature": signature,
            },
            "id": 1,
        });

        self.client.post(&self.url).json(&body).send().await
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
