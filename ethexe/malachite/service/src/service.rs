// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! [`MalachiteService`] — public facade over [`ethexe_malachite_core::MalachiteCore`].
//!
//! Routes ethexe-side inputs (chain heads, injected txs) into the consensus
//! engine and exposes its outputs as a `Stream` of [`MalachiteEvent`]s.

use crate::{
    Mempool,
    externalities::EthexeExternalities,
    mempool::TxInsertionStatus,
    types::{ChainHead, MalachiteEvent},
};
use anyhow::Result;
use ethexe_common::{SimpleBlockData, db::OnChainStorageRO, injected::SignedInjectedTransaction};
use ethexe_malachite_core::MalachiteCore;
use futures::{Stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc::UnboundedReceiver;

/// Public consensus service.
pub struct MalachiteService {
    /// Receiver of outbound events produced by the externalities.
    pub(crate) events_rx: UnboundedReceiver<Result<MalachiteEvent>>,
    /// Latest chain head data, shared with the externalities.
    pub(crate) chain_head: Arc<ChainHead>,
    /// Optional mempool for injected-tx routing; `None` when not a validator.
    pub(crate) mempool: Option<Arc<dyn Mempool>>,
    /// Externalities shared with the inner consensus core.
    pub(crate) externalities: Arc<EthexeExternalities>,
    /// Inner consensus core; `None` after shutdown.
    pub(crate) inner: Option<MalachiteCore<EthexeExternalities>>,
}

impl Drop for MalachiteService {
    fn drop(&mut self) {
        let _ = self.inner.take();
    }
}

impl MalachiteService {
    /// Route an injected transaction into the mempool.
    /// Rejects with `PoolFull` when the node is not a validator.
    pub async fn receive_injected_transaction(
        &self,
        tx: SignedInjectedTransaction,
    ) -> TxInsertionStatus {
        if let Some(pool) = self.mempool.as_ref() {
            pool.insert(tx).await
        } else {
            TxInsertionStatus::PoolFull
        }
    }

    /// Register a newly observed Ethereum block as the chain head.
    pub async fn receive_new_eb(&mut self, eb: SimpleBlockData) {
        let mut current = self.chain_head.latest.write().await;

        // Filter the new head against the current one and update if it's strictly higher
        if eb.header.height > current.header.height {
            *current = eb;
        }
    }

    /// Handle a fully synced Ethereum block: publish it for the producer's
    /// quarantine checks, wake the producer and GC the mempool.
    pub async fn receive_eb_synced(&mut self, eb_hash: H256) {
        let Some(synced) = self.externalities.db.block_simple_data(eb_hash) else {
            tracing::error!(synced = %eb_hash, "synced EB header not found in local DB, ignoring");
            return;
        };

        // Publish the synced head for the externalities' quarantine checks
        // BEFORE waking the producer, so the woken round already sees it.
        // A synced block may lag the latest observed head — advance whenever
        // it is strictly newer than the current synced head.
        {
            let mut latest_synced = self.chain_head.latest_synced.write().await;
            if !latest_synced.hash.is_zero() && synced.header.height <= latest_synced.header.height
            {
                tracing::trace!(
                    latest_synced = %*latest_synced,
                    synced = %synced,
                    "synced EB is not newer than the current synced head"
                );
                drop(latest_synced);
                // Still wake the producer: a lower-height sync may have just
                // landed parent headers a failed descendant walk needs.
                self.chain_head.notify.notify_waiters();
                return;
            }
            *latest_synced = synced;
        }

        // Notify inner proposer if it waits (see EthexeExternalities::wait_for_proposable_content)
        self.chain_head.notify.notify_waiters();

        if let Some(pool) = self.mempool.as_ref() {
            let purged_txs = pool.set_chain_head(synced).await;
            if !purged_txs.is_empty() {
                let event = MalachiteEvent::PurgedTransactions {
                    eb_hash: synced.hash,
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

    /// Handle a prepared Ethereum block: release pending events whose
    /// prerequisite is now satisfied.
    pub async fn receive_eb_prepared(&self, _eb_hash: H256) {
        self.externalities.drain_pending_events().await
    }

    /// Tear down the inner consensus core, releasing its store and sockets.
    pub async fn shutdown(mut self) {
        if let Some(inner) = self.inner.take() {
            inner.shutdown().await;
        }
    }
}

impl Stream for MalachiteService {
    type Item = Result<MalachiteEvent>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // IMPORTANT: MalachiteService async methods (like receive_new_eb and other)
        // are safe to call only because we do not lock any data from self in this method implementation.

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
