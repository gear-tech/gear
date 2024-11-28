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

#[async_trait]
impl TransactionPoolServer for TransactionPoolApi {
    async fn send_message(&self, raw_message: Vec<u8>, signature: Vec<u8>) -> RpcResult<()> {
        log::debug!("Called send_message with vars: raw_message - {raw_message:?}, signature - {signature:?}");

        let (response_sender, response_receiver) = oneshot::channel();
        let input_task = InputTask::AddTransaction {
            transaction: EthexeTransaction::Message {
                raw_message,
                signature,
            },
            response_sender: Some(response_sender),
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
