// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Cross-validator fan-out for injected transactions.
//!
//! [`TransactionsRelayer::relay`] broadcasts a transaction to every
//! validator in the current era and returns the first acceptance.

use crate::{RpcEvent, errors};
use ethexe_common::injected::{InjectedTransactionAcceptance, Transaction};
use jsonrpsee::core::RpcResult;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone)]
pub struct TransactionsRelayer {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
}

impl TransactionsRelayer {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self { rpc_sender }
    }

    /// Broadcast `transaction` to every validator in the current era,
    /// returning the first `Accept` observed by the service.
    pub async fn relay(
        &self,
        transaction: Transaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let tx_hash = transaction.as_ref().hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

        match &transaction {
            Transaction::Injected(transaction) if transaction.data().value != 0 => {
                tracing::warn!(
                    tx_hash = %tx_hash,
                    value = transaction.data().value,
                    "Injected transaction with non-zero value is not supported"
                );
                return Err(errors::bad_request(
                    "Injected transactions with non-zero value are not supported",
                ));
            }
            Transaction::Injected(_) => {}
            Transaction::Shielded(_) => todo!("Shielded transaction relay validation"),
        }

        let (response_sender, response_receiver) = oneshot::channel();
        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            tracing::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        tracing::trace!(%tx_hash, "Relayed transaction, waiting for acceptance");

        response_receiver.await.map_err(|recv_err| {
            tracing::error!(
                ?tx_hash,
                ?recv_err,
                "transaction acceptance channel dropped"
            );

            errors::internal()
        })
    }
}
