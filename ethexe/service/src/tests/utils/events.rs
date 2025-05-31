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
    db::OnChainStorage, events::BlockEvent, tx_pool::SignedOffchainTransaction, SimpleBlockData,
};
use ethexe_compute::{BlockProcessed, ComputeEvent};
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_network::{db_sync, export::PeerId, NetworkEvent};
use ethexe_observer::ObserverEvent;
use ethexe_prometheus::PrometheusEvent;
use ethexe_rpc::RpcEvent;
use gprimitives::H256;
use tokio::sync::{
    broadcast,
    broadcast::{Receiver, Sender},
};

pub type TestingEventSender = Sender<TestingEvent>;
pub type TestingEventReceiver = Receiver<TestingEvent>;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TestingNetworkEvent {
    DbResponse {
        request_id: db_sync::RequestId,
        result: Result<db_sync::Response, db_sync::RequestFailure>,
    },
    DbExternalValidation {
        request_id: db_sync::RequestId,
        response: db_sync::Response,
    },
    Message {
        data: Vec<u8>,
        source: Option<PeerId>,
    },
    PeerBlocked(PeerId),
    PeerConnected(PeerId),
}

impl TestingNetworkEvent {
    fn new(event: &NetworkEvent) -> Self {
        match event {
            NetworkEvent::DbResponse { request_id, result } => Self::DbResponse {
                request_id: *request_id,
                result: result.as_ref().map_err(|(_req, err)| *err).cloned(),
            },
            NetworkEvent::Message { data, source } => Self::Message {
                data: data.clone(),
                source: *source,
            },
            NetworkEvent::PeerBlocked(peer) => Self::PeerBlocked(*peer),
            NetworkEvent::PeerConnected(peer) => Self::PeerConnected(*peer),
        }
    }
}

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
    Network(TestingNetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Prometheus(PrometheusEvent),
    Rpc(TestingRpcEvent),
}

impl TestingEvent {
    pub(crate) fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Network(event) => Self::Network(TestingNetworkEvent::new(event)),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
            Event::Prometheus(event) => Self::Prometheus(event.clone()),
            Event::Rpc(event) => Self::Rpc(TestingRpcEvent::new(event)),
        }
    }
}

pub struct ServiceEventsListener<'a> {
    pub receiver: &'a mut TestingEventReceiver,
}

impl ServiceEventsListener<'_> {
    pub async fn next_event(&mut self) -> anyhow::Result<TestingEvent> {
        self.receiver.recv().await.map_err(Into::into)
    }

    pub async fn wait_for(
        &mut self,
        f: impl Fn(TestingEvent) -> Result<bool>,
    ) -> anyhow::Result<()> {
        self.apply_until(|e| if f(e)? { Ok(Some(())) } else { Ok(None) })
            .await
    }

    pub async fn wait_for_block_processed(&mut self, block_hash: H256) {
        self.wait_for(|event| {
            Ok(matches!(
                event,
                TestingEvent::Compute(ComputeEvent::BlockProcessed(BlockProcessed { block_hash: b })) if b == block_hash
            ))
        }).await.unwrap();
    }

    pub async fn apply_until<R: Sized>(
        &mut self,
        f: impl Fn(TestingEvent) -> Result<Option<R>>,
    ) -> anyhow::Result<R> {
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

            let ObserverEvent::BlockSynced(data) = event else {
                continue;
            };

            let header = OnChainStorage::block_header(&self.db, data.block_hash)
                .expect("Block header not found");
            let events = OnChainStorage::block_events(&self.db, data.block_hash)
                .expect("Block events not found");

            let block_data = SimpleBlockData {
                hash: data.block_hash,
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
