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
use anyhow::Result;
use ethexe_blob_loader::BlobLoaderEvent;
use ethexe_common::{
    Announce, HashOf, SimpleBlockData, db::*, events::BlockEvent, injected::RpcOrNetworkInjectedTx,
};
use ethexe_compute::ComputeEvent;
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_network::NetworkEvent;
use ethexe_observer::ObserverEvent;
use ethexe_rpc::RpcEvent;
use gprimitives::H256;
use std::ops::ControlFlow;
use tokio::sync::broadcast::{self, Receiver, Sender};

pub type TestingEventSender = Sender<TestingEvent>;
pub type TestingEventReceiver = Receiver<TestingEvent>;

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
    Rpc(TestingRpcEvent),
    Fetching,
}

impl TestingEvent {
    pub(crate) fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Network(event) => Self::Network(event.clone()),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
            Event::Rpc(event) => Self::Rpc(TestingRpcEvent::new(event)),
            Event::Fetching(_) => Self::Fetching,
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

    pub async fn wait_for(
        &mut self,
        mut f: impl FnMut(TestingEvent) -> Result<bool>,
    ) -> Result<()> {
        self.apply_until(|e| {
            if f(e)? {
                Ok(ControlFlow::Break(()))
            } else {
                Ok(ControlFlow::Continue(()))
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

        let db = self.db.clone();
        self.apply_until(|event| {
            let TestingEvent::Compute(ComputeEvent::AnnounceComputed(announce_hash)) = event else {
                return Ok(ControlFlow::Continue(()));
            };

            let found = match id {
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
            };

            Ok(found
                .map(ControlFlow::Break)
                .unwrap_or(ControlFlow::Continue(())))
        })
        .await
        .unwrap()
    }

    pub async fn wait_for_announce_rejected(
        &mut self,
        id: impl Into<AnnounceId>,
    ) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce rejected: {id:?}");

        self.apply_until(|event| {
            let TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(announce_hash)) = event
            else {
                return Ok(ControlFlow::Continue(()));
            };

            match id {
                AnnounceId::Any => Ok(ControlFlow::Break(announce_hash)),
                AnnounceId::AnnounceHash(waited_announce_hash) => {
                    if waited_announce_hash == announce_hash {
                        Ok(ControlFlow::Break(announce_hash))
                    } else {
                        Ok(ControlFlow::Continue(()))
                    }
                }
                AnnounceId::BlockHash(_) => unimplemented!("do not support BlockHash here yet"),
            }
        })
        .await
        .unwrap()
    }

    pub async fn wait_for_announce_accepted(
        &mut self,
        id: impl Into<AnnounceId>,
    ) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce accepted: {id:?}");

        let db = self.db.clone();
        self.apply_until(|event| {
            let TestingEvent::Consensus(ConsensusEvent::AnnounceAccepted(announce_hash)) = event
            else {
                return Ok(ControlFlow::Continue(()));
            };

            let found = match id {
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
            };

            Ok(found
                .map(ControlFlow::Break)
                .unwrap_or(ControlFlow::Continue(())))
        })
        .await
        .unwrap()
    }

    pub async fn apply_until<R: Sized>(
        &mut self,
        mut f: impl FnMut(TestingEvent) -> Result<ControlFlow<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;
            if let ControlFlow::Break(res) = f(event)? {
                return Ok(res);
            }
        }
    }

    pub async fn wait_for_block_synced(&mut self) -> H256 {
        self.apply_until(|event| {
            if let TestingEvent::Observer(ObserverEvent::BlockSynced(block_hash)) = event {
                Ok(ControlFlow::Break(block_hash))
            } else {
                Ok(ControlFlow::Continue(()))
            }
        })
        .await
        .unwrap()
    }
}

pub struct ObserverEventsPublisher {
    pub broadcaster: Sender<ObserverEvent>,
    pub db: Database,
}

impl ObserverEventsPublisher {
    pub fn subscribe(&self) -> ObserverEventsListener {
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

    pub async fn apply_until<R: Sized>(
        &mut self,
        mut f: impl FnMut(ObserverEvent) -> Result<ControlFlow<R>>,
    ) -> Result<R> {
        loop {
            let event = self.next_event().await?;
            if let ControlFlow::Break(res) = f(event)? {
                break Ok(res);
            }
        }
    }

    pub async fn apply_until_block<R: Sized>(
        &mut self,
        mut f: impl FnMut(SimpleBlockData) -> Result<ControlFlow<R>>,
    ) -> Result<R> {
        self.apply_until(|event| {
            let ObserverEvent::Block(block_data) = event else {
                return Ok(ControlFlow::Continue(()));
            };

            f(block_data)
        })
        .await
    }

    // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
    // If your test depends on events in skipped blocks, you need to improve this method.
    // TODO #4554: iterate thru skipped blocks.
    pub async fn apply_until_block_event_with_header<R: Sized>(
        &mut self,
        mut f: impl FnMut(BlockEvent, &SimpleBlockData) -> Result<ControlFlow<R>>,
    ) -> Result<R> {
        let db = self.db.clone();
        self.apply_until(|event| {
            let ObserverEvent::BlockSynced(block_hash) = event else {
                return Ok(ControlFlow::Continue(()));
            };

            let header = db.block_header(block_hash).expect("Block header not found");
            let events = db.block_events(block_hash).expect("Block events not found");

            let block_data = SimpleBlockData {
                hash: block_hash,
                header,
            };

            for event in events {
                if let ControlFlow::Break(res) = f(event, &block_data)? {
                    return Ok(ControlFlow::Break(res));
                }
            }

            Ok(ControlFlow::Continue(()))
        })
        .await
    }

    pub async fn apply_until_block_event<R: Sized>(
        &mut self,
        mut f: impl FnMut(BlockEvent) -> Result<ControlFlow<R>>,
    ) -> Result<R> {
        self.apply_until_block_event_with_header(|e, _h| f(e)).await
    }
}
