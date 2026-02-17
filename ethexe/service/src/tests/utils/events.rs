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
    Address, Announce, HashOf, SimpleBlockData,
    db::*,
    events::BlockEvent,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedInjectedTransaction, SignedPromise,
    },
    network::VerifiedValidatorMessage,
};
use ethexe_compute::ComputeEvent;
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_network::{NetworkEvent, NetworkInjectedEvent, export::PeerId};
use ethexe_observer::ObserverEvent;
use ethexe_rpc::RpcEvent;
use futures::{Stream, StreamExt, future::Either, stream, stream::FusedStream};
use gprimitives::H256;
use std::{
    iter,
    pin::Pin,
    task::{Context, Poll, ready},
};

pub type TestingEventSender = EventSender<TestingEvent>;
pub type TestingEventReceiver = EventReceiver<TestingEvent>;
pub type ObserverEventSender = EventSender<ObserverEvent>;
pub type ObserverEventReceiver = EventReceiver<ObserverEvent>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestingNetworkInjectedEvent {
    InboundTransaction {
        transaction: SignedInjectedTransaction,
    },
    OutboundAcceptance {
        transaction_hash: HashOf<InjectedTransaction>,
        acceptance: InjectedTransactionAcceptance,
    },
}

impl TestingNetworkInjectedEvent {
    fn new(event: &NetworkInjectedEvent) -> Self {
        match event {
            NetworkInjectedEvent::InboundTransaction {
                transaction,
                channel: _,
            } => Self::InboundTransaction {
                transaction: transaction.clone(),
            },
            NetworkInjectedEvent::OutboundAcceptance {
                transaction_hash,
                acceptance,
            } => Self::OutboundAcceptance {
                transaction_hash: *transaction_hash,
                acceptance: acceptance.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestingNetworkEvent {
    ValidatorMessage(VerifiedValidatorMessage),
    PromiseMessage(SignedPromise),
    ValidatorIdentityUpdated(Address),
    InjectedTransaction(TestingNetworkInjectedEvent),
    PeerBlocked(PeerId),
    PeerConnected(PeerId),
}

impl TestingNetworkEvent {
    fn new(event: &NetworkEvent) -> Self {
        match event {
            NetworkEvent::ValidatorMessage(message) => Self::ValidatorMessage(message.clone()),
            NetworkEvent::PromiseMessage(message) => Self::PromiseMessage(message.clone()),
            NetworkEvent::ValidatorIdentityUpdated(address) => {
                Self::ValidatorIdentityUpdated(*address)
            }
            NetworkEvent::InjectedTransaction(event) => {
                Self::InjectedTransaction(TestingNetworkInjectedEvent::new(event))
            }
            NetworkEvent::PeerBlocked(peer_id) => Self::PeerBlocked(*peer_id),
            NetworkEvent::PeerConnected(peer_id) => Self::PeerConnected(*peer_id),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TestingRpcEvent {
    InjectedTransaction {
        transaction: AddressedInjectedTransaction,
    },
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
    Network(TestingNetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Rpc(TestingRpcEvent),
    Fetching,
}

impl TestingEvent {
    pub fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Network(event) => Self::Network(TestingNetworkEvent::new(event)),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
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

pub trait InfiniteStreamExt: StreamExt + Sized + Unpin {
    #[must_use]
    async fn find_map<U>(&mut self, mut f: impl FnMut(Self::Item) -> Option<U>) -> U {
        loop {
            let item = self.next().await.expect("always Some");
            if let Some(res) = f(item) {
                return res;
            }
        }
    }

    async fn find(&mut self, mut f: impl FnMut(&Self::Item) -> bool) -> Self::Item {
        self.find_map(|item| if f(&item) { Some(item) } else { None })
            .await
    }
}

impl<T: StreamExt + Sized + Unpin> InfiniteStreamExt for T {}

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

impl<T: Clone> Stream for EventReceiver<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let res = ready!(Pin::new(&mut self.inner).poll_recv(cx));
            match res {
                Some(Ok(event)) => break Poll::Ready(Some(event)),
                None | Some(Err(RecvError::Closed)) => panic!("service unexpectedly closed"),
                Some(Err(RecvError::Overflowed(skipped))) => {
                    tracing::trace!("channel overflowed and skipped {skipped} events");
                    continue;
                }
            }
        }
    }
}

impl<T: Clone> FusedStream for EventReceiver<T> {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl<T: Clone> EventReceiver<T> {
    pub fn new_receiver(&self) -> Self {
        let inner = self.inner.new_receiver();
        let db = self.db.clone();
        Self { inner, db }
    }
}

impl TestingEventReceiver {
    async fn find_announce<F>(&mut self, id: AnnounceId, event_to_hash: F) -> HashOf<Announce>
    where
        F: Fn(TestingEvent) -> Option<HashOf<Announce>>,
    {
        let db = self.db.clone();
        self.find_map(|event| {
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

    pub async fn find_announce_computed(&mut self, id: impl Into<AnnounceId>) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce computed: {id:?}");
        self.find_announce(id, |event| {
            if let TestingEvent::Compute(ComputeEvent::AnnounceComputed(computed_data)) = event {
                Some(computed_data.announce_hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn find_announce_rejected(&mut self, id: impl Into<AnnounceId>) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce rejected: {id:?}");
        self.find_announce(id, |event| {
            if let TestingEvent::Consensus(ConsensusEvent::AnnounceRejected(hash)) = event {
                Some(hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn find_announce_accepted(&mut self, id: impl Into<AnnounceId>) -> HashOf<Announce> {
        let id = id.into();
        log::info!("📗 waiting for announce accepted: {id:?}");
        self.find_announce(id, |event| {
            if let TestingEvent::Consensus(ConsensusEvent::AnnounceAccepted(hash)) = event {
                Some(hash)
            } else {
                None
            }
        })
        .await
    }

    pub async fn find_block_synced(&mut self) -> H256 {
        self.find_map(|event| {
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
    pub fn filter_map_block(self) -> impl Stream<Item = SimpleBlockData> {
        self.filter_map(|event| async move {
            if let ObserverEvent::Block(block_data) = event {
                Some(block_data)
            } else {
                None
            }
        })
    }

    // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
    // If your test depends on events in skipped blocks, you need to improve this method.
    // TODO #4554: iterate thru skipped blocks.
    pub fn filter_map_block_synced_with_header(
        self,
    ) -> impl Stream<Item = (BlockEvent, SimpleBlockData)> {
        let db = self.db.clone();
        self.flat_map(move |event| {
            let ObserverEvent::BlockSynced(block_hash) = event else {
                return Either::Left(stream::empty());
            };

            let header = db.block_header(block_hash).expect("Block header not found");
            let events = db.block_events(block_hash).expect("Block events not found");

            let block_data = SimpleBlockData {
                hash: block_hash,
                header,
            };

            Either::Right(stream::iter(
                events.into_iter().zip(iter::repeat(block_data)),
            ))
        })
    }

    pub fn filter_map_block_synced(self) -> impl Stream<Item = BlockEvent> {
        self.filter_map_block_synced_with_header()
            .map(|(event, _)| event)
    }
}
