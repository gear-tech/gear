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

use crate::Event;
use anyhow::{Result, anyhow};
use ethexe_blob_loader::BlobLoaderEvent;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData, db::*, events::BlockEvent,
    tx_pool::SignedOffchainTransaction,
};
use ethexe_compute::ComputeEvent;
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_network::NetworkEvent;
use ethexe_observer::ObserverEvent;
use ethexe_prometheus::PrometheusEvent;
use ethexe_rpc::RpcEvent;
use ethexe_tx_pool::TxPoolEvent;
use gprimitives::H256;
use tokio::sync::{
    broadcast,
    broadcast::{Receiver, Sender},
};

pub type TestingEventSender = Sender<TestingEvent>;
pub type TestingEventReceiver = Receiver<TestingEvent>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestingRpcEvent {
    OffchainTransaction {
        transaction: SignedOffchainTransaction,
    },
}

impl TestingRpcEvent {
    fn new(event: &RpcEvent) -> Self {
        match event {
            RpcEvent::OffchainTransaction {
                transaction,
                response_sender: _,
            } => Self::OffchainTransaction {
                transaction: transaction.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TestingEvent {
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
    TxPool(TxPoolEvent),
}

impl TestingEvent {
    pub(crate) fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Network(event) => Self::Network(event.clone()),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
            Event::Prometheus(event) => Self::Prometheus(event.clone()),
            Event::Rpc(event) => Self::Rpc(TestingRpcEvent::new(event)),
            Event::TxPool(event) => Self::TxPool(event.clone()),
        }
    }
}

pub struct ServiceEventsListener<'a> {
    pub receiver: &'a mut TestingEventReceiver,
    pub db: Database,
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

impl ServiceEventsListener<'_> {
    pub async fn next_event(&mut self) -> Result<TestingEvent> {
        self.receiver.recv().await.map_err(Into::into)
    }

    pub async fn wait_for(&mut self, f: impl Fn(TestingEvent) -> Result<bool>) -> Result<()> {
        self.apply_until(|e| if f(e)? { Ok(Some(())) } else { Ok(None) })
            .await
    }

    pub async fn wait_for_announce_computed(&mut self, id: impl Into<AnnounceId>) {
        let id = id.into();
        loop {
            let event = self.next_event().await.unwrap();
            let TestingEvent::Compute(ComputeEvent::AnnounceComputed(announce_hash)) = event else {
                continue;
            };

            match id {
                AnnounceId::Any => {
                    return;
                }
                AnnounceId::AnnounceHash(waited_announce_hash)
                    if waited_announce_hash == announce_hash =>
                {
                    return;
                }
                AnnounceId::BlockHash(waited_block_hash) => {
                    if self
                        .db
                        .announce(announce_hash)
                        .ok_or_else(|| {
                            anyhow!("Announce {announce_hash} not found in listener's node DB")
                        })
                        .unwrap()
                        .block_hash
                        == waited_block_hash
                    {
                        return;
                    }
                }
                _ => continue,
            }
        }
    }

    pub async fn apply_until<R: Sized>(
        &mut self,
        f: impl Fn(TestingEvent) -> Result<Option<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;
            if let Some(res) = f(event)? {
                return Ok(res);
            }
        }
    }
}

pub struct ObserverEventsPublisher {
    pub broadcaster: Sender<ObserverEvent>,
    pub db: Database,
}

impl ObserverEventsPublisher {
    pub async fn subscribe(&self) -> ObserverEventsListener {
        ObserverEventsListener {
            receiver: self.broadcaster.subscribe(),
            db: self.db.clone(),
        }
    }
}

pub struct ObserverEventsListener {
    receiver: broadcast::Receiver<ObserverEvent>,
    db: Database,
}

impl Clone for ObserverEventsListener {
    fn clone(&self) -> Self {
        Self {
            receiver: self.receiver.resubscribe(),
            db: self.db.clone(),
        }
    }
}

impl ObserverEventsListener {
    pub async fn next_event(&mut self) -> Result<ObserverEvent> {
        self.receiver.recv().await.map_err(Into::into)
    }

    #[allow(unused)]
    pub async fn apply_until<R: Sized>(
        &mut self,
        mut f: impl FnMut(ObserverEvent) -> Result<Option<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;
            if let Some(res) = f(event)? {
                return Ok(res);
            }
        }
    }

    pub async fn apply_until_block_event<R: Sized>(
        &mut self,
        mut f: impl FnMut(BlockEvent) -> Result<Option<R>>,
    ) -> Result<R> {
        self.apply_until_block_event_with_header(|e, _h| f(e)).await
    }

    // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
    // If your test depends on events in skipped blocks, you need to improve this method.
    // TODO #4554: iterate thru skipped blocks.
    pub async fn apply_until_block_event_with_header<R: Sized>(
        &mut self,
        mut f: impl FnMut(BlockEvent, &SimpleBlockData) -> Result<Option<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;

            let ObserverEvent::BlockSynced(block_hash) = event else {
                continue;
            };

            let header = self
                .db
                .block_header(block_hash)
                .expect("Block header not found");
            let events = self
                .db
                .block_events(block_hash)
                .expect("Block events not found");

            let block_data = SimpleBlockData {
                hash: block_hash,
                header,
            };

            for event in events {
                if let Some(res) = f(event, &block_data)? {
                    return Ok(res);
                }
            }
        }
    }
}
