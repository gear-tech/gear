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

//! Cross-validator fan-out for injected transactions.
//!
//! [`TransactionsRelayer::relay`] broadcasts a transaction to every
//! validator in the current era and returns the first acceptance.
//
// TODO +_+_+: master had three unit tests pinning the previous
// `calculate_next_producer` routing math (`test_calculate_next_producer_return_next`,
// `_return_next_next`, `_in_next_era`); the underlying function was
// deleted in favor of the fan-out path above. The new strategy
// (`current_validators` + `fan_out`) has no unit coverage at all —
// add at minimum `test_fan_out_uses_all_validators_in_era`,
// `test_fan_out_falls_back_to_send_single_when_validators_empty`,
// `test_fan_out_returns_last_reject_when_no_accept`. Setup mirrors
// the master tests' `setup_db` + `set_validators(0, ...)` helpers.

use crate::{RpcEvent, errors};
use anyhow::Context as _;
use ethexe_common::{
    Address, ValidatorsVec,
    db::{ConfigStorageRO, OnChainStorageRO},
    injected::{AddressedInjectedTransaction, InjectedTransactionAcceptance},
};
use ethexe_db::Database;
use futures::{StreamExt, stream::FuturesUnordered};
use jsonrpsee::core::RpcResult;
use std::time::{Duration, SystemTime, SystemTimeError};
use tokio::sync::{mpsc, oneshot};

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
    /// Falls back to a single-recipient delivery using the original
    /// `transaction.recipient` if the validator set isn't known yet.
    pub async fn relay(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let tx_hash = transaction.tx.data().to_hash();
        tracing::trace!(%tx_hash, ?transaction, "Called injected_sendTransaction with vars");

        if transaction.tx.data().value != 0 {
            tracing::warn!(
                tx_hash = %tx_hash,
                value = transaction.tx.data().value,
                "Injected transaction with non-zero value is not supported"
            );
            return Err(errors::bad_request(
                "Injected transactions with non-zero value are not supported",
            ));
        }

        let recipients: Vec<Address> = current_validators(&self.db)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();

        if recipients.is_empty() {
            return self.send_single(transaction, tx_hash).await;
        }

        self.fan_out(transaction, &recipients, tx_hash).await
    }

    async fn send_single(
        &self,
        transaction: AddressedInjectedTransaction,
        tx_hash: ethexe_common::HashOf<ethexe_common::injected::InjectedTransaction>,
    ) -> RpcResult<InjectedTransactionAcceptance> {
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

        tracing::trace!(%tx_hash, "Accept transaction, waiting for promise");

        response_receiver.await.map_err(|e| {
            tracing::error!(
                "Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}"
            );
            errors::internal()
        })
    }

    async fn fan_out(
        &self,
        transaction: AddressedInjectedTransaction,
        recipients: &[Address],
        tx_hash: ethexe_common::HashOf<ethexe_common::injected::InjectedTransaction>,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        let mut response_futures = FuturesUnordered::new();
        for recipient in recipients {
            let (response_sender, response_receiver) = oneshot::channel();
            let event = RpcEvent::InjectedTransaction {
                transaction: AddressedInjectedTransaction {
                    recipient: *recipient,
                    tx: transaction.tx.clone(),
                },
                response_sender,
            };

            if let Err(err) = self.rpc_sender.send(event) {
                tracing::error!(
                    "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                    The receiving end in the main service might have been dropped."
                );
                return Err(errors::internal());
            }

            response_futures.push(response_receiver);
        }

        tracing::trace!(%tx_hash, "Broadcast transaction, waiting for first acceptance");

        let mut last_reject: Option<InjectedTransactionAcceptance> = None;
        while let Some(result) = response_futures.next().await {
            match result {
                Ok(InjectedTransactionAcceptance::Accept) => {
                    return Ok(InjectedTransactionAcceptance::Accept);
                }
                Ok(rejection) => last_reject = Some(rejection),
                Err(_) => {}
            }
        }

        last_reject.map(Ok).unwrap_or_else(|| {
            tracing::error!(
                %tx_hash,
                "All response senders for the `RpcEvent::InjectedTransaction` fan-out were dropped"
            );
            Err(errors::internal())
        })
    }
}

/// Validator set effective right now. Errors propagate when the
/// protocol timelines aren't configured yet or when the era's
/// validator vector is missing — callers fall back to single-recipient
/// delivery in that case.
fn current_validators(db: &Database) -> anyhow::Result<ValidatorsVec> {
    let timelines = db.config().timelines;
    let now = now_since_unix_epoch()
        .context("system clock error")?
        .as_secs();
    let era = timelines
        .era_from_ts(now)
        .context("failed to calculate era from current timestamp")?;
    db.validators(era)
        .with_context(|| format!("validators not found for era={era}"))
}

fn now_since_unix_epoch() -> Result<Duration, SystemTimeError> {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)
}
