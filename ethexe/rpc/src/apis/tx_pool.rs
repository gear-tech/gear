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
use ethexe_tx_pool::{InputTask, RawTransacton, SignedTransaction, Transaction, TxPoolSender};
use gprimitives::{H160, H256};
use jsonrpsee::{
    core::{async_trait, RpcResult},
    proc_macros::rpc,
};
use tokio::sync::oneshot;

#[rpc(server)]
pub trait TransactionPool {
    #[method(name = "transactionPool_sendMessage")]
    async fn send_message(
        &self,
        program_id: H160,
        payload: Vec<u8>,
        value: u128,
        reference_block: H256,
        signature: Vec<u8>,
    ) -> RpcResult<H256>;
}

#[derive(Clone)]
pub struct TransactionPoolApi {
    tx_pool_sender: TxPoolSender,
}

impl TransactionPoolApi {
    pub fn new(tx_pool_sender: TxPoolSender) -> Self {
        Self { tx_pool_sender }
    }
}

#[async_trait]
impl TransactionPoolServer for TransactionPoolApi {
    async fn send_message(
        &self,
        program_id: H160,
        payload: Vec<u8>,
        value: u128,
        reference_block: H256,
        signature: Vec<u8>,
    ) -> RpcResult<H256> {
        let signed_ethexe_tx = SignedTransaction {
            transaction: Transaction {
                raw: RawTransacton::SendMessage {
                    program_id,
                    payload,
                    value,
                },
                reference_block,
            },
            signature,
        };
        log::debug!("Called send_message with vars: {signed_ethexe_tx:#?}");

        let (response_sender, response_receiver) = oneshot::channel();
        let input_task = InputTask::AddTransaction {
            transaction: signed_ethexe_tx,
            response_sender: Some(response_sender),
        };

        self.tx_pool_sender.send(input_task).map_err(|e| {
            // No panic case as a responsibility of the RPC API is fulfilled.
            // The dropped tx pool input task receiver might signalize that
            // the transaction pool has been stooped.
            log::error!(
                "Failed to send tx pool add transaction input task: {e}. \
                The receiving end in the tx pool might have been dropped."
            );
            errors::internal()
        })?;

        let res = response_receiver.await.map_err(|e| {
            // No panic case as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the transaction pool
            // has crashed or is malformed, so problems should be handled there.
            log::error!("Tx pool has dropped response sender: {e}");
            errors::internal()
        })?;

        res.map_err(errors::tx_pool)
    }
}
