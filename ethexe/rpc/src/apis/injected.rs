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

use crate::{RpcEvent, errors};
use ethexe_common::injected::SignedInjectedTransaction;
use jsonrpsee::{
    core::{RpcResult, async_trait},
    proc_macros::rpc,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc::UnboundedSender, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
}

#[rpc(server)]
pub trait Injected {
    #[method(name = "injected_sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: SignedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance>;
}

#[derive(Debug, Clone)]
pub struct InjectedApi {
    rpc_sender: UnboundedSender<RpcEvent>,
}

impl InjectedApi {
    pub(crate) fn new(rpc_sender: UnboundedSender<RpcEvent>) -> Self {
        Self { rpc_sender }
    }
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: SignedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        log::debug!("Called send_message with vars: {transaction:?}");

        let (response_sender, response_receiver) = oneshot::channel();
        self.rpc_sender
            .send(RpcEvent::InjectedTransaction {
                transaction,
                response_sender,
            })
            .map_err(|e| {
                // That could be a panic case, as rpc_receiver must not be dropped,
                // but the main service works independently from rpc and can be malformed.
                log::error!(
                    "Failed to send `RpcEvent::InjectedTransaction` event task: {e}. \
                    The receiving end in the main service might have been dropped."
                );
                errors::internal()
            })?;

        response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            log::error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}");
            errors::internal()
        })
    }
}
