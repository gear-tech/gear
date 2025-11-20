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
use anyhow::Result;
use dashmap::DashMap;
use ethexe_common::{
    Address, HashOf, ProtocolTimelines,
    db::{LatestData, LatestDataStorageRO, OnChainStorageRO},
    injected::{InjectedTransaction, RpcOrNetworkInjectedTx, SignedPromise, VALIDITY_WINDOW},
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use jsonrpsee::{
    PendingSubscriptionSink, SubscriptionMessage,
    core::{RpcResult, SubscriptionResult, async_trait},
    proc_macros::rpc,
    types::ErrorObjectOwned,
};
use serde::{Deserialize, Serialize};
use sp_core::H256;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectedTransactionAcceptance {
    Accept,
}

#[cfg_attr(not(feature = "test-utils"), rpc(server))]
#[cfg_attr(feature = "test-utils", rpc(server, client))]
pub trait Injected {
    #[method(name = "injected_sendTransaction")]
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance>;

    #[subscription(
        name = "injected_subscribeTransactionPromise",
        unsubscribe = "injected_unsubscribeTransactionPromise", 
        item = SignedPromise
    )]
    async fn send_transaction_and_watch(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult;
}

#[derive(Debug, Clone)]
pub struct InjectedApi {
    gate: TxSafetyGate,
    rpc_sender: mpsc::UnboundedSender<RpcEvent>,
    promise_waiters: Arc<DashMap<HashOf<InjectedTransaction>, oneshot::Sender<SignedPromise>>>,
}

impl InjectedApi {
    pub(crate) fn new(db: Database, rpc_sender: mpsc::UnboundedSender<RpcEvent>) -> Self {
        Self {
            gate: TxSafetyGate::new_with_all_checks(db),
            rpc_sender,
            promise_waiters: Arc::new(DashMap::new()),
        }
    }
}

impl InjectedApi {
    pub fn send_promise(&self, promise: SignedPromise) {
        let Some((_, promise_sender)) = self.promise_waiters.remove(&promise.data().tx_hash) else {
            tracing::warn!(promise = ?promise, "receive unregistered promise");
            return;
        };

        if let Err(promise) = promise_sender.send(promise) {
            tracing::trace!(promise = ?promise, "rpc promise receiver dropped");
        }
    }
}

#[async_trait]
impl InjectedServer for InjectedApi {
    async fn send_transaction(
        &self,
        transaction: RpcOrNetworkInjectedTx,
    ) -> RpcResult<InjectedTransactionAcceptance> {
        tracing::trace!("Called injected_sendTransaction with vars: {transaction:?}");

        self.gate.pass_transaction(&transaction)?;

        let (response_sender, response_receiver) = oneshot::channel();
        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            log::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal());
        }

        response_receiver.await.map_err(|e| {
            // No panic case, as a responsibility of the RPC API is fulfilled.
            // The dropped sender signalizes that the main service has crashed
            // or is malformed, so problems should be handled there.
            log::error!("Response sender for the `RpcEvent::InjectedTransaction` was dropped: {e}");
            errors::internal()
        })
    }

    async fn send_transaction_and_watch(
        &self,
        pending: PendingSubscriptionSink,
        transaction: RpcOrNetworkInjectedTx,
    ) -> SubscriptionResult {
        tracing::trace!("Called injected_subscribeTransactionPromise with vars: {transaction:?}");
        self.gate.pass_transaction(&transaction)?;

        let tx_hash = transaction.tx.data().to_hash();

        // Checks, that transaction wasn't already send.
        if self.promise_waiters.get(&tx_hash).is_some() {
            tracing::warn!(tx_hash = ?tx_hash, "transaction was already sent");
            return Err(
                format!("transaction with the same hash was already sent: {tx_hash}").into(),
            );
        }

        let (response_sender, response_receiver) = oneshot::channel();
        let (promise_sender, promise_receiver) = oneshot::channel();

        let event = RpcEvent::InjectedTransaction {
            transaction,
            response_sender,
        };

        if let Err(err) = self.rpc_sender.send(event) {
            log::error!(
                "Failed to send `RpcEvent::InjectedTransaction` event task: {err}. \
                The receiving end in the main service might have been dropped."
            );
            return Err(errors::internal().into());
        }

        let _accepted = response_receiver.await?;

        let subscription_sink = match pending.accept().await {
            Ok(sink) => sink,
            Err(err) => {
                tracing::warn!(
                    "failed to accept subscription for injected transaction promise: {err}"
                );
                return Ok(());
            }
        };

        self.promise_waiters.insert(tx_hash, promise_sender);

        tokio::spawn(async move {
            let Ok(promise) = promise_receiver.await else {
                return;
            };

            let promise_msg = match SubscriptionMessage::from_json(&promise) {
                Ok(msg) => msg,
                Err(err) => {
                    tracing::error!(
                        error = %err,
                        "failed to create `SubscriptionMessage` from json object"
                    );
                    return;
                }
            };

            if let Err(err) = subscription_sink.send(promise_msg).await {
                tracing::warn!(
                    tx_hash = ?tx_hash,
                    error = %err,
                    "failed to send subscription message"
                )
            }
        });

        Ok(())
    }
}

const REFERENCE_BLOCK_OUTDATED_TOLERANCE: u8 = 1;

// Implement tower service for InjectedApi and use this as layer.
#[derive(Clone, derive_more::Debug)]
pub(crate) struct TxSafetyGate<DB = Database> {
    #[debug(skip)]
    db: DB,
    config: TxSafetyChecksConfig,
    protocol_timelines: ProtocolTimelines,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TxSafetyChecksConfig {
    recipient: bool,
    reference_block: bool,
}

impl<DB: OnChainStorageRO> TxSafetyGate<DB> {
    pub fn new(db: DB) -> Self {
        let protocol_timelines = db
            .protocol_timelines()
            .expect("Not found protocol timelines in database. Unable to run RPC server.");

        Self {
            db,
            config: TxSafetyChecksConfig::default(),
            protocol_timelines,
        }
    }

    fn new_with_all_checks(db: DB) -> Self {
        Self::new(db)
            .with_recipient_check()
            .with_reference_block_check()
    }

    pub fn with_recipient_check(mut self) -> Self {
        self.config.recipient = true;
        self
    }

    pub fn with_reference_block_check(mut self) -> Self {
        self.config.reference_block = true;
        self
    }
}

impl<DB: Storage + LatestDataStorageRO + OnChainStorageRO + Clone> TxSafetyGate<DB> {
    /// Validate obviously incorrect transactions. It it is invalid returns error message.
    fn pass_transaction(&self, tx: &RpcOrNetworkInjectedTx) -> Result<(), ErrorObjectOwned> {
        let Some(latest_data) = self.db.latest_data() else {
            return Err(errors::db("not found latest data in RPC database"));
        };

        if self.config.recipient {
            self.recipient_exists(&latest_data, &tx.recipient)?;
        }

        if self.config.reference_block {
            self.ref_block_outdated(&latest_data, tx.tx.data().reference_block)?;
        }

        Ok(())
    }

    fn recipient_exists(
        &self,
        latest_data: &LatestData,
        recipient: &Address,
    ) -> Result<(), ErrorObjectOwned> {
        let current_era = self
            .protocol_timelines
            .era_from_ts(latest_data.synced_block.header.timestamp);

        // TODO kuzmindev: consider to add hashing validators by era.
        let Some(validators) = self.db.validators(current_era) else {
            tracing::warn!(era = %current_era, "not found validators");
            return Err(errors::db("not found validators in RPC database"));
        };

        if !validators.contains(recipient) {
            return Err(errors::bad_request(format!(
                "there is no validator with address {recipient}"
            )));
        }

        Ok(())
    }

    fn ref_block_outdated(
        &self,
        latest_data: &LatestData,
        reference_block: H256,
    ) -> Result<(), ErrorObjectOwned> {
        let Some(reference_block) = self.db.block_header(reference_block) else {
            return Err(errors::bad_request(format!(
                "not found reference block in RPC database, block hash {reference_block}"
            )));
        };

        let Some(ref_block_lagging) = latest_data
            .synced_block
            .header
            .height
            .checked_sub(reference_block.height)
        else {
            // Probably never happen.
            return Ok(());
        };

        if ref_block_lagging > (REFERENCE_BLOCK_OUTDATED_TOLERANCE + VALIDITY_WINDOW).into() {
            return Err(errors::bad_request("reference block is outdated".into()));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ethexe_common::{
        Address, SimpleBlockData,
        db::{LatestDataStorageRW, OnChainStorageRW},
        ecdsa::PrivateKey,
        injected::SignedInjectedTransaction,
        mock::Mock,
    };

    use super::*;

    #[test]
    fn test_gate_reject_invalid_recipient() {
        let db = Database::memory();

        // Preparing db
        let reference_block = SimpleBlockData::mock(());
        let mut latest_data = LatestData::default();
        latest_data.synced_block = reference_block.clone();
        db.set_latest_data(latest_data);

        let timelines = ProtocolTimelines::mock(());
        db.set_protocol_timelines(timelines);

        let validators = vec![Address::from(1), Address::from(2)];
        db.set_validators(
            timelines.era_from_ts(reference_block.header.timestamp),
            validators.clone().try_into().unwrap(),
        );

        // Test
        let gate = TxSafetyGate::new(db.clone()).with_recipient_check();

        let mut tx = InjectedTransaction::mock(());
        tx.reference_block = reference_block.hash;
        let signed_tx =
            SignedInjectedTransaction::create(PrivateKey::random(), tx.clone()).unwrap();

        let invalid_rpc_tx = RpcOrNetworkInjectedTx {
            recipient: Address::from(3),
            tx: signed_tx.clone(),
        };
        assert!(gate.pass_transaction(&invalid_rpc_tx).is_err());

        let correct_rpc_tx = RpcOrNetworkInjectedTx {
            recipient: validators[0],
            tx: signed_tx,
        };
        assert!(gate.pass_transaction(&correct_rpc_tx).is_ok());
    }
}
