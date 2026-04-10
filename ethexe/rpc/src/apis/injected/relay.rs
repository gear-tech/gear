// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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
use ethexe_common::injected::{AddressedInjectedTransaction, InjectedTransactionAcceptance};
use jsonrpsee::core::RpcResult;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, trace, warn};

#[derive(Clone)]
pub struct TransactionsRelayer {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
}

impl TransactionsRelayer {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self { rpc_sender }
    }

    pub async fn relay(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let tx_hash = transaction.tx.data().to_hash();
        trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

        // TODO: maybe should implement the transaction validator.
        if transaction.tx.data().value != 0 {
            warn!(
                tx_hash = %tx_hash,
                value = transaction.tx.data().value,
                "Injected transaction with non-zero value is not supported"
            );
            return Err(errors::bad_request(
                "Injected transactions with non-zero value are not supported",
            ));
        }

        let (response_sender, response_receiver) = oneshot::channel();
        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        trace!(%tx_hash, "Accept transaction, waiting for promise");

        response_receiver.await.map_err(|err| {
            // Expecting no errors here, because the rpc channel is owned by main server.
            error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {err}");
            errors::internal()
        })
    }
}
