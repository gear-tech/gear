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
