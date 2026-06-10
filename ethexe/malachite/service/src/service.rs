// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`MalachiteService`] — public facade.
//!
//! Wraps [`ethexe_malachite_core::MalachiteService`] with the ethexe-shaped API the
//! rest of the workspace already consumes. Owns:
//!
//! - the chain-head register that [`Self::receive_new_chain_head`]
//!   updates and [`crate::EthexeExternalities`] reads,
//! - the [`Mempool`] handle that serves both injected-tx routing and
//!   the producer's content selection,
//! - the inner [`ethexe_malachite_core::MalachiteService`] itself, polled inline so
//!   any `Err` item surfaces on this service's stream and so
//!   [`Self::shutdown`] can `await` the engine actor's full teardown
//!   (releasing the RocksDB advisory lock before
//!   re-opening on the same home directory).

use crate::{
    MalachiteEvent, Mempool, ValidatorEntry, externalities::EthexeExternalities,
    mempool::TxInsertionStatus, types::ChainHead,
};
use anyhow::Result;
use ethexe_common::{
    Address, SimpleBlockData,
    db::{ConfigStorageRO, OnChainStorageRO},
    injected::SignedInjectedTransaction,
};
use ethexe_malachite_core::MalachiteCore;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use gsigner::schemes::secp256k1::PublicKey;
use std::{
    collections::HashMap,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc::UnboundedReceiver;

/// Public consensus service.
pub struct MalachiteService {
    pub(crate) events_rx: UnboundedReceiver<Result<MalachiteEvent>>,
    pub(crate) chain_head: Arc<ChainHead>,
    pub(crate) mempool: Option<Arc<dyn Mempool>>,
    pub(crate) externalities: Arc<EthexeExternalities>,
    pub(crate) validators: HashMap<Address, PublicKey>,
    pub(crate) active_era: u64,
    pub(crate) inner: Option<MalachiteCore<EthexeExternalities>>,
}

impl Drop for MalachiteService {
    fn drop(&mut self) {
        let _ = self.inner.take();
    }
}

impl MalachiteService {
    pub fn receive_injected_transaction(&self, tx: SignedInjectedTransaction) -> TxInsertionStatus {
        self.mempool
            .as_ref()
            .map(|pool| pool.insert(tx))
            .unwrap_or(TxInsertionStatus::PoolFull)
    }

    pub async fn receive_new_chain_head(&mut self, head: SimpleBlockData) {
        let mut current = self.chain_head.latest.write().await;

        // Filter the new head against the current one and update if it's strictly higher
        if head.header.height > current.header.height {
            *current = head;
        }
    }

    pub async fn receive_eb_synced(&mut self, eb_hash: H256) {
        let chain_head = *self.chain_head.latest.read().await;
        if chain_head.hash != eb_hash {
            tracing::trace!(
                chain_head = %chain_head,
                synced = %eb_hash,
                "synced EB is not the current chain head, ignoring"
            );
            return;
        }

        // Publish the synced head for the externalities' quarantine
        // checks BEFORE waking the producer, so the woken round
        // already sees it.
        *self.chain_head.latest_synced.write().await = chain_head;

        // Notify inner proposer if it waits (see EthexeExternalities::wait_for_proposable_content)
        self.chain_head.notify.notify_one();

        // Rotate before waking the producer so the next round sees the new set.
        self.maybe_rotate_validators_for_era(chain_head);

        if let Some(pool) = self.mempool.as_ref() {
            let purged_txs = pool.set_chain_head(chain_head);
            if !purged_txs.is_empty() {
                let event = MalachiteEvent::PurgedTransactions {
                    eb_hash: chain_head.hash,
                    transactions: purged_txs,
                };
                if let Err(err) = self.externalities.event_tx.send(Ok(event)) {
                    tracing::error!(
                        event = ?err.0,
                        "malachite-service: event_tx channel closed, failed to send purged transactions event"
                    );
                };
            }
        }
    }

    pub async fn receive_eb_prepared(&self, _eb_hash: H256) {
        self.externalities.drain_pending_events().await
    }

    /// Push the on-chain validators for `head`'s era into the engine,
    /// if the era moved. Skips on missing DB data or unknown pub keys
    /// (wait-and-retry: the next `BlockSynced` re-evaluates).
    fn maybe_rotate_validators_for_era(&mut self, head: SimpleBlockData) {
        let db = &self.externalities.db;
        let timelines = db.config().timelines;
        let Some(era) = timelines.era_from_ts(head.header.timestamp) else {
            return;
        };
        if self.active_era == era {
            return;
        }
        let Some(addrs) = db.validators(era) else {
            // trace like error because `head` must be synced
            tracing::error!(era, "validators for era not yet in DB; deferring rotation");
            return;
        };

        let mut new_set = Vec::with_capacity(self.validators.len());
        let mut missing: Vec<Address> = Vec::new();
        for addr in addrs.iter() {
            match self.validators.get(addr) {
                Some(pk) => new_set.push(ValidatorEntry {
                    public_key: *pk,
                    voting_power: 1,
                }),
                None => missing.push(*addr),
            }
        }

        if !missing.is_empty() {
            tracing::warn!(
                era,
                missing = ?missing,
                "validator pool missing pub keys for some on-chain era validators; \
                 keeping the previous active set",
            );
            return;
        }

        // Bug-class failure — advance active_era so we don't loop on the same broken input.
        let inner = match self.inner.as_ref() {
            Some(inner) => inner,
            None => {
                tracing::error!(era, "rotate after shutdown");
                self.active_era = era;
                return;
            }
        };

        if let Err(e) = inner.update_validators(new_set) {
            tracing::error!(era, error = %e, "rotating malachite validator set failed");
            self.active_era = era;
            return;
        }

        self.active_era = era;

        tracing::info!(
            era,
            "rotated malachite validator set to era's on-chain quorum"
        );
    }

    pub async fn shutdown(mut self) {
        if let Some(inner) = self.inner.take() {
            inner.shutdown().await;
        }
    }
}

impl Stream for MalachiteService {
    type Item = Result<MalachiteEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(inner) = self.inner.as_mut() {
            match Pin::new(&mut *inner).poll_next(cx) {
                Poll::Ready(Some(e)) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    self.inner = None;
                }
                Poll::Pending => {}
            }
        }
        self.events_rx.poll_recv(cx)
    }
}

impl FusedStream for MalachiteService {
    fn is_terminated(&self) -> bool {
        self.events_rx.is_closed()
    }
}
