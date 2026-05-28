// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::{RpcEvent, errors, metrics::InjectedApiMetrics};

use super::{
    InjectedServer, promise_manager::PromiseSubscriptionManager, relay::TransactionsRelayer,
    spawner,
};
use ethexe_common::{
    HashOf,
    db::InjectedStorageRO,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedTxReceipt,
    },
};
use ethexe_db::Database;
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult, async_trait},
    server::PendingSubscriptionSink,
};
use std::ops::Deref;
use tokio::sync::mpsc;
use tracing::trace;

const MAX_TRANSACTION_IDS: usize = 100;

#[derive(Clone)]
pub struct InjectedApi {
    db: Database,
    manager: PromiseSubscriptionManager,
    relayer: TransactionsRelayer,
    metrics: InjectedApiMetrics,
}

// TODO: Issue #5387
#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        self.send_transaction(transaction).await
    }

    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: AddressedInjectedTransaction,
    ) -> SubscriptionResult {
        self.send_transaction_and_watch(pending, transaction).await
    }

    async fn get_transaction_receipt(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedTxReceipt>> {
        self.get_transaction_receipt(tx_hash).await
    }

    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> RpcResult<Vec<Option<SignedInjectedTransaction>>> {
        self.get_transactions(transaction_ids).await
    }
}

impl Deref for InjectedApi {
    type Target = PromiseSubscriptionManager;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl InjectedApi {
    pub fn new(db: Database, rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            db: db.clone(),
            manager: PromiseSubscriptionManager::new(db.clone()),
            relayer: TransactionsRelayer::new(rpc_sender, db),
            metrics: InjectedApiMetrics::default(),
        }
    }
}

// RPC API implementation.
impl InjectedApi {
    async fn send_transaction(
        &self,
        transaction: AddressedInjectedTransaction,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        self.relayer.relay(transaction).await
    }

    // TODO: Issue #5386.
    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: AddressedInjectedTransaction,
    ) -> SubscriptionResult {
        let tx_hash = transaction.tx.data().to_hash();

        let pending_subscriber = match self.manager.try_register_subscriber(tx_hash) {
            Ok(subscriber) => subscriber,
            Err(err) => {
                return Err(errors::bad_request(err).into());
            }
        };

        let acceptance = self.relayer.relay(transaction).await.inspect_err(|_err| {
            self.manager.cancel_registration(tx_hash);
        })?;
        let sink = match acceptance {
            InjectedTransactionAcceptance::Accept => {
                pending.accept().await.inspect_err(|_err| {
                    self.manager.cancel_registration(tx_hash);
                })?
            }
            InjectedTransactionAcceptance::AlreadyPooled { reason } => {
                // Promise will fire normally; keep the subscription so a
                // retry / duplicate submit doesn't lose the reply.
                tracing::debug!(%tx_hash, reason, "watch: retaining subscription on duplicate");
                pending.accept().await.inspect_err(|_err| {
                    self.manager.cancel_registration(tx_hash);
                })?
            }
            InjectedTransactionAcceptance::Reject { reason } => {
                self.manager.cancel_registration(tx_hash);
                return Err(reason.into());
            }
        };

        self.metrics.injected_tx_active_subscriptions.increment(1);
        let (manager, metrics) = (self.manager.clone(), self.metrics.clone());
        spawner::spawn_pending_subscriber(sink, pending_subscriber, move |tx_hash| {
            manager.cancel_registration(tx_hash);
            metrics.injected_tx_active_subscriptions.decrement(1);
        });
        Ok(())
    }

    async fn get_transaction_receipt(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedTxReceipt>> {
        match self.db.receipt(tx_hash) {
            Some(receipt) => Ok(Some(receipt)),
            None => {
                trace!(?tx_hash, "receipt not found for injected transaction");
                Ok(None)
            }
        }
    }

    async fn get_transactions(
        &self,
        transaction_ids: Vec<HashOf<InjectedTransaction>>,
    ) -> RpcResult<Vec<Option<SignedInjectedTransaction>>> {
        tracing::trace!(?transaction_ids, "Called injected_getTransactions");

        if transaction_ids.len() > MAX_TRANSACTION_IDS {
            return Err(errors::invalid_params(format!(
                "Too many transaction ids requested. Maximum is {MAX_TRANSACTION_IDS}.",
            )));
        }

        let transactions = transaction_ids
            .into_iter()
            .map(|tx_id| self.db.injected_transaction(tx_id))
            .collect::<Vec<Option<SignedInjectedTransaction>>>();

        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        PrivateKey, SignedMessage,
        db::InjectedStorageRW,
        injected::{Promise, Receipt},
        mock::Mock,
    };

    fn make_signed_tx() -> SignedInjectedTransaction {
        SignedInjectedTransaction::create(PrivateKey::random(), InjectedTransaction::mock(()))
            .expect("creating signed injected transaction succeeds")
    }

    fn make_injected_api(db: Database) -> InjectedApi {
        let (sender, _receiver) = mpsc::unbounded_channel();
        InjectedApi::new(db, sender)
    }

    #[tokio::test]
    async fn test_get_transactions_found() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx = make_signed_tx();
        let tx_hash = tx.data().to_hash();
        db.set_injected_transaction(tx.clone());

        let result = api.get_transactions(vec![tx_hash]).await.unwrap();
        assert_eq!(result, vec![Some(tx)]);
    }

    #[tokio::test]
    async fn test_get_transactions_not_found() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx_hash = make_signed_tx().data().to_hash();
        // Transaction not stored in DB.
        let result = api.get_transactions(vec![tx_hash]).await.unwrap();
        assert_eq!(result, vec![None]);
    }

    #[tokio::test]
    async fn test_get_transactions_mixed() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let tx1 = make_signed_tx();
        let tx2 = make_signed_tx();
        let hash1 = tx1.data().to_hash();
        let hash2 = tx2.data().to_hash();
        db.set_injected_transaction(tx1.clone());
        // tx2 not stored.

        let result = api.get_transactions(vec![hash1, hash2]).await.unwrap();
        assert_eq!(result, vec![Some(tx1), None]);
    }

    #[tokio::test]
    async fn test_get_transactions_empty() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let result = api.get_transactions(vec![]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_get_transactions_exceeds_limit() {
        let db = Database::memory();
        let api = make_injected_api(db.clone());

        let ids = (0..=MAX_TRANSACTION_IDS)
            .map(|_| make_signed_tx().data().to_hash())
            .collect();

        let result = api.get_transactions(ids).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_transaction_receipt_returns_stored_receipt() {
        let db = Database::memory();
        let api = make_injected_api(db);

        let tx_hash = make_signed_tx().data().to_hash();
        let promise = Promise::mock(tx_hash);
        let compact_receipt =
            SignedMessage::create(PrivateKey::random(), Receipt::Promise(promise.to_compact()))
                .expect("creating signed receipt succeeds")
                .into();

        api.on_computed_promise(promise.clone());
        api.on_tx_receipt(compact_receipt);

        let receipt = api
            .get_transaction_receipt(tx_hash)
            .await
            .expect("RPC result succeeds")
            .expect("receipt is stored");

        assert_eq!(receipt.data(), &Receipt::Promise(promise));
    }

    #[tokio::test]
    async fn test_get_transaction_receipt_ignores_unsigned_promise_body() {
        let db = Database::memory();
        let api = make_injected_api(db);

        let tx_hash = make_signed_tx().data().to_hash();
        api.on_computed_promise(Promise::mock(tx_hash));

        let receipt = api
            .get_transaction_receipt(tx_hash)
            .await
            .expect("RPC result succeeds");

        assert_eq!(receipt, None);
    }
}
