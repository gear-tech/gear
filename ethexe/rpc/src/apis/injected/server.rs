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
    HashOf, SignedMessage,
    db::InjectedStorageRO,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedPromise,
    },
};
use ethexe_db::Database;
use jsonrpsee::{
    core::{RpcResult, SubscriptionResult, async_trait},
    server::PendingSubscriptionSink,
};
use std::ops::Deref;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct InjectedApi {
    db: Database,
    manager: PromiseSubscriptionManager,
    relayer: TransactionsRelayer,
}

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

    async fn get_transaction(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<SignedInjectedTransaction> {
        self.get_transaction(tx_hash).await
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
            manager: PromiseSubscriptionManager::new(db),
            relayer: TransactionsRelayer::new(rpc_sender),
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

        let pending_watcher = match self.manager.try_register_watcher(tx_hash) {
            Ok(watcher) => watcher,
            Err(err) => {
                self.manager.cancel_registration(tx_hash);
                return Err(errors::bad_request(err).into());
            }
        };

        let sink = match self.relayer.relay(transaction).await? {
            InjectedTransactionAcceptance::Accept => pending.accept().await?,
            InjectedTransactionAcceptance::Reject { reason } => {
                self.manager.cancel_registration(tx_hash);
                return Err(reason.into());
            }
        };

        let watchers = self.manager.watchers();
        spawner::spawn_pending_subscription(sink, pending_watcher, move |tx_hash| {
            watchers.remove(&tx_hash);
        });
        Ok(())
    }

    async fn get_transaction_promise(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<Option<SignedPromise>> {
        if self.db.injected_transaction(tx_hash).is_some() {
            // TODO: add error message here
            return Err(errors::bad_request(""));
        }

        let Some(promise) = self.db.promise(tx_hash) else {
            tracing::trace!(?tx_hash, "promise not found for injected transaction");
            return Ok(None);
        };

        let Some((signature, address)) = self.db.promise_signature(tx_hash) else {
            tracing::trace!(
                ?tx_hash,
                "promise signature not found for injected transaction"
            );
            return Ok(None);
        };

        match SignedMessage::try_from_parts(promise, signature, address) {
            Ok(message) => Ok(Some(message)),
            Err(err) => {
                tracing::trace!(
                    ?tx_hash,
                    ?err,
                    "failed to build signed promise from parts for injected transaction"
                );
                Ok(None)
            }
        }
    }

    async fn get_transaction(
        &self,
        tx_hash: HashOf<InjectedTransaction>,
    ) -> RpcResult<SignedInjectedTransaction> {
        let Some(tx) = self.db.injected_transaction(tx_hash) else {
            return Err(errors::not_found());
        };

        Ok(tx)
    }
}
