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
use jsonrpsee::types::ErrorObject;
use reqwest::{Client, Response, Result};
use gprimitives::{H160, H256};
use sp_core::Bytes;
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

    pub async fn calculate_reply_for_handle(
        &self,
        at: Option<H256>,
        source: H160,
        program_id: H160,
        payload: Bytes,
        value: u128,
    ) -> Result<Response> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "program_calculateReplyForHandle",
            "params": {
                "at": at,
                "source": source,
                "program_id": program_id,
                "payload": payload,
                "value": value,
            },
            "id": 1,
        });

        self.client.post(&self.url).json(&body).send().await
    }
}

#[derive(Deserialize)]
pub struct SerdeJsonRpcResponse {
    inner: serde_json::Value,
}

impl SerdeJsonRpcResponse {
    pub async fn new(response: Response) -> Result<Self> {
        let inner = response.json().await?;

        Ok(Self { inner })
    }

    pub fn try_extract_res<T: DeserializeOwned>(&self) -> AnyhowResult<T> {
        match self.inner.get("result") {
            Some(result) => {
                let result: T = serde_json::from_value(result.clone())?;

                Ok(result)
            }
            None => bail!("No 'result' found in response"),
        }
    }

    pub fn try_extract_err(&self) -> AnyhowResult<ErrorObject<'static>> {
        match self.inner.get("error") {
            Some(error) => {
                let error: ErrorObject<'static> = serde_json::from_value(error.clone())?;

                Ok(error)
            }
            None => bail!("No 'error' found in response"),
        }
    }
}
