// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Cross-validator fan-out for injected transactions.
//!
//! [`TransactionsRelayer::relay`] broadcasts a transaction to every
//! validator in the current era and returns the first acceptance.
//!
//! Concurrent calls for the same transaction hash share a single in-flight
//! relay — one service event, one network broadcast, one Accept/Reject
//! outcome observed by every caller. The in-flight entry is removed once the
//! outcome is published, so a later resubmission relays afresh.

use crate::{RpcEvent, errors};
use dashmap::{DashMap, mapref::entry::Entry};
use ethexe_common::{
    HashOf,
    injected::{InjectedTransaction, InjectedTransactionAcceptance, SignedInjectedTransaction},
};
use jsonrpsee::core::RpcResult;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, watch};

/// `None` until the shared relay resolves with the service's answer.
type RelayOutcome = Option<RpcResult<InjectedTransactionAcceptance>>;
type InFlightRelays = Arc<DashMap<HashOf<InjectedTransaction>, watch::Receiver<RelayOutcome>>>;

#[derive(Debug, Clone)]
pub struct TransactionsRelayer {
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    in_flight: InFlightRelays,
}

impl TransactionsRelayer {
    pub fn new(rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            rpc_sender,
            in_flight: InFlightRelays::default(),
        }
    }

    /// Broadcast `transaction` to every validator in the current era,
    /// returning the first `Accept` observed by the service.
    ///
    /// Deduplicated per transaction hash: while a relay for this hash is in
    /// flight, concurrent callers await its outcome instead of relaying again.
    pub async fn relay(
        &self,
        transaction: SignedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
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

        let mut outcome_rx = match self.in_flight.entry(tx_hash) {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let (outcome_tx, outcome_rx) = watch::channel(None);
                entry.insert(outcome_rx.clone());

                let rpc_sender = self.rpc_sender.clone();
                let in_flight = Arc::clone(&self.in_flight);
                // Detached so caller cancellation cannot strand other waiters:
                // the task always publishes the outcome or drops the sender.
                tokio::spawn(async move {
                    let result = relay_to_service(rpc_sender, transaction, tx_hash).await;
                    // Remove before publishing so a later resubmission relays afresh.
                    in_flight.remove(&tx_hash);
                    let _ = outcome_tx.send(Some(result));
                });
                outcome_rx
            }
        };

        let outcome = outcome_rx
            .wait_for(|outcome| outcome.is_some())
            .await
            .map_err(|_relay_task_gone| errors::internal())?;
        outcome
            .as_ref()
            .expect("`wait_for` guarantees the outcome is set")
            .clone()
    }
}

/// Sends the transaction to the main service and awaits its acceptance.
async fn relay_to_service(
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    transaction: SignedInjectedTransaction,
    tx_hash: HashOf<InjectedTransaction>,
) -> RpcResult<InjectedTransactionAcceptance> {
    let (response_sender, response_receiver) = oneshot::channel();
    let event = RpcEvent::InjectedTransaction {
        transaction,
        response_sender,
    };

    if let Err(err) = rpc_sender.send(event) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{SignedMessage, ecdsa::PrivateKey, mock::Mock};

    fn mock_signed_transaction() -> SignedInjectedTransaction {
        SignedMessage::create(PrivateKey::random(), InjectedTransaction::mock(())).unwrap()
    }

    #[tokio::test]
    async fn concurrent_relays_of_same_transaction_share_one_relay() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let relayer = TransactionsRelayer::new(sender);
        let tx = mock_signed_transaction();

        let responder = tokio::spawn(async move {
            let RpcEvent::InjectedTransaction {
                response_sender, ..
            } = receiver.recv().await.expect("relay event");
            response_sender
                .send(InjectedTransactionAcceptance::Accept)
                .expect("response receiver remains open");

            let second =
                tokio::time::timeout(std::time::Duration::from_millis(200), receiver.recv()).await;
            assert!(
                second.is_err(),
                "same in-flight transaction must not relay twice"
            );
        });

        let (first, second) = tokio::join!(relayer.relay(tx.clone()), relayer.relay(tx));
        assert_eq!(first.unwrap(), InjectedTransactionAcceptance::Accept);
        assert_eq!(second.unwrap(), InjectedTransactionAcceptance::Accept);
        responder.await.unwrap();
    }

    #[tokio::test]
    async fn completed_relay_does_not_cache_outcome() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let relayer = TransactionsRelayer::new(sender);
        let tx = mock_signed_transaction();

        // Two sequential relays must produce two service events.
        let responder = tokio::spawn(async move {
            for _ in 0..2 {
                let RpcEvent::InjectedTransaction {
                    response_sender, ..
                } = receiver.recv().await.expect("relay event");
                response_sender
                    .send(InjectedTransactionAcceptance::Accept)
                    .expect("response receiver remains open");
            }
        });

        relayer.relay(tx.clone()).await.unwrap();
        relayer.relay(tx).await.unwrap();
        responder.await.expect("both relays reached the service");
    }
}
