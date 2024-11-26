// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

//! Transaction pool rpc interface.

use crate::errors;
use ethexe_tx_pool::{EthexeTransaction, InputTask, TxPoolInputTaskSender};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use tokio::sync::oneshot;

#[rpc(server)]
pub trait TransactionPool {
    #[method(name = "transactionPool_sendMessage")]
    async fn send_message(&self, raw_message: Vec<u8>, signature: Vec<u8>) -> RpcResult<()>;
}

#[derive(Clone)]
pub struct TransactionPoolApi {
    tx_pool_task_sender: TxPoolInputTaskSender<EthexeTransaction>,
}

impl TransactionPoolApi {
    pub fn new(tx_pool_task_sender: TxPoolInputTaskSender<EthexeTransaction>) -> Self {
        Self {
            tx_pool_task_sender,
        }
    }
}

/*
Test case for raw rpc call with websocat:
transactionPool_sendMessage {"raw_message": [104, 101, 108, 108, 111, 95, 119, 111, 114, 108, 100], "signature": [89, 105, 98, 73, 40, 32, 213, 19, 244, 171, 227, 144, 199, 56, 94, 1, 223, 229, 208, 245, 103, 132, 35, 75, 99, 195, 70, 169, 1, 48, 250, 219, 100, 79, 7, 240, 253, 122, 22, 12, 87, 45, 173, 191, 36, 72, 136, 222, 57, 6, 55, 244, 196, 125, 135, 250, 237, 70, 116, 65, 46, 175, 75, 37, 27]}
*/

#[async_trait]
impl TransactionPoolServer for TransactionPoolApi {
    async fn send_message(&self, raw_message: Vec<u8>, signature: Vec<u8>) -> RpcResult<()> {
        log::debug!("Called send_transaction with vars: raw_message - {raw_message:?}, signature - {signature:?}");

        let (response_sender, response_receiver) = oneshot::channel();
        let input_task = InputTask::AddTransaction {
            transaction: EthexeTransaction::Message {
                raw_message,
                signature,
            },
            response_sender,
        };

        self.tx_pool_task_sender.send(input_task).map_err(|e| {
            log::error!(
                "Failed to send tx pool input task: {e}. \
                The receiving end in the tx pool might have been dropped."
            );
            errors::internal()
        })?;

        let res = response_receiver.await.map_err(|e| {
            log::error!("Failed to receive tx pool response: {e}");
            errors::internal()
        })?;

        res.map_err(errors::tx_pool)
    }
}
