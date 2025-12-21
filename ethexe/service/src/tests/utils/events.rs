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

#![allow(clippy::double_parens)] // produced by `derive_more::TryUnwrap`

use crate::Event;
use async_broadcast::{Receiver, RecvError, Sender};
use ethexe_blob_loader::BlobLoaderEvent;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData, db::*, events::BlockEvent, injected::RpcOrNetworkInjectedTx,
};
use ethexe_compute::ComputeEvent;
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_network::NetworkEvent;
use ethexe_observer::ObserverEvent;
use ethexe_prometheus::PrometheusEvent;
use ethexe_rpc::RpcEvent;
use gprimitives::H256;

pub type TestingEventSender = EventSender<TestingEvent>;
pub type TestingEventReceiver = EventReceiver<TestingEvent>;
pub type ObserverEventSender = EventSender<ObserverEvent>;
pub type ObserverEventReceiver = EventReceiver<ObserverEvent>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestingRpcEvent {
    InjectedTransaction { transaction: RpcOrNetworkInjectedTx },
}

impl TestingRpcEvent {
    fn new(event: &RpcEvent) -> Self {
        match event {
            RpcEvent::InjectedTransaction {
                transaction,
                response_sender: _,
            } => Self::InjectedTransaction {
                transaction: transaction.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::TryUnwrap)]
pub enum TestingEvent {
    // Fast sync done. Sent just once.
    FastSyncDone(H256),
    // Basic event to notify that service has started. Sent just once.
    ServiceStarted,
    // Services events.
    Compute(ComputeEvent),
    Consensus(ConsensusEvent),
    Network(NetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Prometheus(PrometheusEvent),
    Rpc(TestingRpcEvent),
    Fetching,
}

impl TestingEvent {
    pub fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Network(event) => Self::Network(event.clone()),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
            Event::Prometheus(event) => Self::Prometheus(event.clone()),
            Event::Rpc(event) => Self::Rpc(TestingRpcEvent::new(event)),
            Event::Fetching(_) => Self::Fetching,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, derive_more::From)]
pub enum AnnounceId {
    /// Wait for any next computed announce
    #[default]
    Any,
    /// Wait for announce computed with a specific hash
    AnnounceHash(HashOf<Announce>),
    /// Wait for announce computed with a specific block hash
    BlockHash(H256),
}

pub fn channel<T>(db: Database) -> (EventSender<T>, EventReceiver<T>) {
    let (mut tx, rx) = async_broadcast::broadcast(1024);
    tx.set_overflow(true);
    (EventSender { inner: tx }, EventReceiver { inner: rx, db })
}

#[derive(Debug, Clone)]
pub struct EventSender<T> {
    inner: Sender<T>,
}

impl<T: Clone> EventSender<T> {
    pub async fn send(&self, event: T) {
        self.inner.broadcast_direct(event).await.unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct EventReceiver<T> {
    inner: Receiver<T>,
    db: Database,
}

impl<T: Clone> EventReceiver<T> {
    pub fn new_receiver(&self) -> Self {
        let inner = self.inner.new_receiver();
        let db = self.db.clone();
        Self { inner, db }
    }

    async fn recv(&mut self) -> T {
        loop {
            match self.inner.recv_direct().await {
                Ok(event) => break event,
                Err(RecvError::Closed) => panic!("service unexpectedly closed"),
                Err(RecvError::Overflowed(skipped)) => {
                    tracing::trace!("channel overflowed and skipped {skipped} events");
                    continue;
                }
            }
        }
    }

    #[must_use]
    pub async fn wait_map<U>(&mut self, mut f: impl FnMut(T) -> Option<U>) -> U {
        loop {
            let event = self.recv().await;
            if let Some(res) = f(event) {
                return res;
            }
        }
    }

    pub async fn wait(&mut self, mut f: impl FnMut(&T) -> bool) -> T {
        self.wait_map(|e| f(&e).then_some(e)).await
    }
}

impl TestingEventReceiver {
    async fn wait_for_announce<F>(&mut self, id: AnnounceId, event_to_hash: F) -> HashOf<Announce>
    where
        F: Fn(TestingEvent) -> Option<HashOf<Announce>>,
    {
        let db = self.db.clone();
        self.wait_map(|event| {
            let announce_hash = event_to_hash(event)?;

            match id {
                AnnounceId::Any => Some(announce_hash),
                AnnounceId::AnnounceHash(waited_announce_hash) => {
                    (waited_announce_hash == announce_hash).then_some(announce_hash)
                }
                AnnounceId::BlockHash(block_hash) => db
                    .announce(announce_hash)
                    .unwrap_or_else(|| {
                        panic!("Accepted announce {announce_hash} not found in listener's node DB")
                    })
                    .block_hash
                    .eq(&block_hash)
                    .then_some(announce_hash),
            }
        })
        .await
    }

    pub async fn wait_for_announce_computed(
        &mut self,
        id: impl Into<AnnounceId>,
    ) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce computed: {id:?}");
        self.wait_for_announce(id, |event| {
            if let TestingEvent::Compute(ComputeEvent::AnnounceComputed(hash)) = event {
                Some(hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn wait_for_announce_rejected(
        &mut self,
        id: impl Into<AnnounceId>,
    ) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce rejected: {id:?}");
        self.wait_for_announce(id, |event| {
            if let TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(hash)) = event {
                Some(hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn wait_for_announce_accepted(
        &mut self,
        id: impl Into<AnnounceId>,
    ) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce accepted: {id:?}");

        self.wait_for_announce(id, |event| {
            if let TestingEvent::Consensus(ConsensusEvent::AnnounceAccepted(hash)) = event {
                Some(hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn wait_for_block_synced(&mut self) -> H256 {
        self.wait_map(|event| {
            if let TestingEvent::Observer(ObserverEvent::BlockSynced(block_hash)) = event {
                Some(block_hash)
            } else {
                None
            }
        })
        .await
    }
}

impl ObserverEventReceiver {
    pub async fn wait_map_block<U>(
        &mut self,
        mut f: impl FnMut(SimpleBlockData) -> Option<U>,
    ) -> U {
        self.wait_map(|event| {
            let ObserverEvent::Block(block_data) = event else {
                return None;
            };

            f(block_data)
        })
        .await
    }

    pub async fn wait_block(
        &mut self,
        mut f: impl FnMut(SimpleBlockData) -> bool,
    ) -> SimpleBlockData {
        self.wait_map_block(|data| f(data).then_some(data)).await
    }

    // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
    // If your test depends on events in skipped blocks, you need to improve this method.
    // TODO #4554: iterate thru skipped blocks.
    pub async fn wait_map_block_synced_with_header<U>(
        &mut self,
        mut f: impl FnMut(BlockEvent, SimpleBlockData) -> Option<U>,
    ) -> U {
        let db = self.db.clone();
        self.wait_map(|event| {
            let ObserverEvent::BlockSynced(block_hash) = event else {
                return None;
            };

            let header = db.block_header(block_hash).expect("Block header not found");
            let events = db.block_events(block_hash).expect("Block events not found");

            let block_data = SimpleBlockData {
                hash: block_hash,
                header,
            };

            for event in events {
                if let Some(res) = f(event, block_data) {
                    return Some(res);
                }
            }

            None
        })
        .await
    }

    pub async fn wait_block_synced_with_header(
        &mut self,
        mut f: impl FnMut(&BlockEvent, SimpleBlockData) -> bool,
    ) -> (BlockEvent, SimpleBlockData) {
        self.wait_map_block_synced_with_header(|e, d| f(&e, d).then_some((e, d)))
            .await
    }

    pub async fn wait_map_block_synced<U>(
        &mut self,
        mut f: impl FnMut(BlockEvent) -> Option<U>,
    ) -> U {
        self.wait_map_block_synced_with_header(|e, _h| f(e)).await
    }

    pub async fn wait_block_synced(
        &mut self,
        mut f: impl FnMut(&BlockEvent) -> bool,
    ) -> BlockEvent {
        self.wait_map_block_synced(|event| f(&event).then_some(event))
            .await
    }
}
