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

//! Transaction pool rpc interface.

use crate::{RpcEvent, errors};
use ethexe_common::{
    ecdsa::Signature,
    injected::{InjectedTransaction, SignedInjectedTransaction},
};
use gprimitives::H256;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};
use parity_scale_codec::Decode;
use tokio::sync::{mpsc, oneshot};

// TODO: REMOVE TX POOL

#[rpc(server)]
pub trait TransactionPool {
    #[method(name = "transactionPool_sendMessage")]
    async fn send_message(
        &self,
        injected_tx: InjectedTransaction,
        signature: Vec<u8>,
    ) -> RpcResult<H256>;
}

#[derive(Clone)]
pub struct TransactionPoolApi {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
}

impl TransactionPoolApi {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self { rpc_sender }
    }
}

#[async_trait]
impl TransactionPoolServer for TransactionPoolApi {
    async fn send_message(
        &self,
        injected_tx: InjectedTransaction,
        signature: Vec<u8>,
    ) -> RpcResult<H256> {
        let signature = Signature::decode(&mut signature.as_slice()).map_err(|e| {
            log::error!("Failed to decode signature: {e}");
            errors::internal()
        })?;

        let signed_injected_tx = SignedInjectedTransaction::try_from_parts(injected_tx, signature)
            .map_err(|e| {
                log::error!("{e}");
                errors::internal()
            })?;

        log::debug!("Called send_message with vars: {signed_injected_tx:#?}");

        let (response_sender, response_receiver) = oneshot::channel();
        self.rpc_sender
            .send(RpcEvent::InjectedTransaction {
                transaction: signed_injected_tx,
                response_sender,
            })
            .map_err(|e| {
                // That could be a panic case, as rpc_receiver must not be dropped,
                // but the main service works independently from rpc and can be malformed.
                log::error!(
                    "Failed to send `RpcEvent::OffchainTransaction` event task: {e}. \
                    The receiving end in the main service might have been dropped."
                );
                errors::internal()
            })?;

        let res = response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            log::error!("Response sender for the `RpcEvent::OffchainTransaction` was dropped: {e}");
            errors::internal()
        })?;

        todo!()
    }
}
