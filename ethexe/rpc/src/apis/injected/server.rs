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

use super::{
    InjectedServer, promise_manager::PromiseSubscriptionManager, relay::TransactionsRelayer,
    spawner,
};
use ethexe_common::{
    HashOf,
    db::InjectedStorageRO,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedPromise, restore_signed_promise,
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
}

// TODO: add metrics middleware for InjectedApi
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

    async fn get_transaction_promise(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedPromise>> {
        self.get_transaction_promise(tx_hash).await
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
            InjectedTransactionAcceptance::Reject { reason } => {
                self.manager.cancel_registration(tx_hash);
                return Err(reason.into());
            }
        };

        let manager = self.manager.clone();
        spawner::spawn_pending_subscriber(sink, pending_subscriber, move |tx_hash| {
            manager.cancel_registration(tx_hash);
        });
        Ok(())
    }

    async fn get_transaction_promise(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedPromise>> {
        let Some(promise) = self.db.promise(tx_hash) else {
            trace!(?tx_hash, "promise not found for injected transaction");
            return Ok(None);
        };

        let Some(compact) = self.db.compact_promise(tx_hash) else {
            trace!(
                ?tx_hash,
                "compact promise not found for injected transaction"
            );
            return Ok(None);
        };

        match restore_signed_promise(promise, &compact) {
            Ok(message) => Ok(Some(message)),
            Err(err) => {
                trace!(
                    ?tx_hash,
                    ?err,
                    "failed to build signed promise from parts for injected transaction"
                );
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
    use ethexe_common::{PrivateKey, db::InjectedStorageRW, mock::Mock};

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
}
