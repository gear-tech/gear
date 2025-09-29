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

use gearexe_common::tx_pool::OffchainTransaction;
use reqwest::{Client, Response, Result};

/// Client for the gearexe rpc server.
pub struct RpcClient {
    client: Client,
    url: String,
}

impl RpcClient {
    pub fn new(url: String) -> Self {
        let client = Client::new();

        Self { client, url }
    }

    /// Send message using transaction pool API (`transactionPool_sendMessage`) of the gearexe rpc server.
    pub async fn send_message(
        &self,
        gearexe_tx: OffchainTransaction,
        signature: Vec<u8>,
    ) -> Result<Response> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "transactionPool_sendMessage",
            "params": {
                "gearexe_tx": gearexe_tx,
                "signature": signature,
            },
            "id": 1,
        });

        self.client.post(&self.url).json(&body).send().await
    }
}
