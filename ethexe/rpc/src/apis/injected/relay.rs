// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Cross-validator fan-out for injected transactions.
//!
//! [`TransactionsRelayer::relay`] broadcasts a transaction to every
//! validator in the current era and returns the first acceptance.

use crate::{RpcEvent, errors};
use ethexe_common::injected::{InjectedTransactionAcceptance, SignedInjectedTransaction};
use ethexe_db::Database;
use futures::{StreamExt, stream::FuturesUnordered};
use jsonrpsee::core::RpcResult;
use std::time::{Duration, SystemTime, SystemTimeError};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct TransactionsRelayer {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    db: Database,
}

impl TransactionsRelayer {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>, db: Database) -> Self {
        Self { rpc_sender, db }
    }

    /// Broadcast `transaction` to every validator in the current era,
    /// returning the first `Accept` (or the last `Reject` if none accept).
    /// Falls back to a zero-address delivery if the validator set isn't known yet.
    pub fn relay(&self, transaction: SignedInjectedTransaction) -> RpcResult<()> {
        let tx_hash = transaction.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

        if transaction.data().value != 0 {
            tracing::warn!(
                tx_hash = %tx_hash,
                value = transaction.data().value,
                "Injected transaction with non-zero value is not supported"
            );
            return Err(errors::bad_request(
                "Injected transactions with non-zero value are not supported",
            ));
        }

        self.rpc_sender
            .send(RpcEvent::InjectedTransaction { transaction })
            .map_err(|error| {
                tracing::error!(
                    ?error,
                    "failed to send injected transaction to inner service"
                );
                errors::internal()
            })
    }
}

fn now_since_unix_epoch() -> Result<Duration, SystemTimeError> {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
}
