// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

#![allow(clippy::double_parens)] // produced by `derive_more::TryUnwrap`

use crate::Event;
use alloy::providers::{RootProvider, ext::AnvilApi};
use async_broadcast::{Receiver, RecvError, Sender};
use ethexe_blob_loader::BlobLoaderEvent;
use ethexe_common::{
    Address, HashOf, SimpleBlockData,
    db::*,
    events::BlockEvent,
    injected::{
        AddressedInjectedTransaction, InjectedTransaction, InjectedTransactionAcceptance,
        SignedCompactTxReceipt, SignedInjectedTransaction,
    },
    network::VerifiedValidatorMessage,
};
use ethexe_compute::ComputeEvent;
use ethexe_consensus::ConsensusEvent;
use ethexe_db::Database;
use ethexe_malachite::MalachiteEvent;
use ethexe_network::{NetworkEvent, NetworkInjectedEvent, export::PeerId};
use ethexe_observer::ObserverEvent;
use ethexe_rpc::RpcEvent;
use futures::{
    FutureExt, Stream, StreamExt,
    future::{self, BoxFuture, Either},
    stream::{self, BoxStream, FusedStream},
};
use gprimitives::H256;
use std::{
    iter,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};

pub type TestingEventSender = EventSender<TestingEvent>;
pub type TestingEventReceiver = KickingStream<EventReceiver<TestingEvent>>;
pub type ObserverEventSender = EventSender<ObserverEvent>;
pub type ObserverEventReceiver = KickingStream<EventReceiver<ObserverEvent>>;

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
                peer: _,
                transaction,
                channel: _,
            } => Self::InboundTransaction {
                transaction: SignedInjectedTransaction::clone(transaction),
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
    TxReceiptMessage(SignedCompactTxReceipt),
    ValidatorIdentityUpdated(Address),
    InjectedTransaction(TestingNetworkInjectedEvent),
    PeerBlocked(PeerId),
    PeerConnected(PeerId),
}

impl TestingNetworkEvent {
    fn new(event: &NetworkEvent) -> Self {
        match event {
            NetworkEvent::ValidatorMessage(message) => Self::ValidatorMessage(message.clone()),
            NetworkEvent::TxReceiptMessage(message) => Self::TxReceiptMessage(message.clone()),
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
    #[allow(dead_code)]
    FastSyncDone(H256),
    // Basic event to notify that service has started. Sent just once.
    ServiceStarted,
    // Services events.
    Compute(ComputeEvent),
    Consensus(ConsensusEvent),
    Malachite(MalachiteEvent),
    Network(TestingNetworkEvent),
    Observer(ObserverEvent),
    BlobLoader(BlobLoaderEvent),
    Rpc(TestingRpcEvent),
    Prometheus,
}

impl TestingEvent {
    pub fn new(event: &Event) -> Self {
        match event {
            Event::Compute(event) => Self::Compute(event.clone()),
            Event::Consensus(event) => Self::Consensus(event.clone()),
            Event::Malachite(event) => Self::Malachite(event.clone()),
            Event::Network(event) => Self::Network(TestingNetworkEvent::new(event)),
            Event::Observer(event) => Self::Observer(event.clone()),
            Event::BlobLoader(event) => Self::BlobLoader(event.clone()),
            Event::Rpc(event) => Self::Rpc(TestingRpcEvent::new(event)),
            Event::Prometheus(_event) => Self::Prometheus,
        }
    }
}

pub trait KickExt {
    fn kick(&self) -> BoxFuture<'static, ()>;
    fn take_kicks(&mut self) -> Option<(Duration, RootProvider)>;
}

#[derive(Debug, Clone)]
pub struct KickingStream<S> {
    inner: S,
    kicks: Option<(Duration, RootProvider)>,
}

impl<S> KickingStream<S> {
    pub fn new(inner: S, kicks: Option<(Duration, RootProvider)>) -> Self {
        Self { inner, kicks }
    }
}

impl<S: Stream + Unpin> Stream for KickingStream<S> {
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}

impl<S: FusedStream + Unpin> FusedStream for KickingStream<S> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<S> KickExt for KickingStream<S> {
    fn kick(&self) -> BoxFuture<'static, ()> {
        if let Some((duration, provider)) = &self.kicks {
            let provider = provider.clone();
            let duration = *duration;
            async move {
                tokio::time::sleep(duration).await;
                log::info!("⏱️ Reached kicking timeout, forcing new block");
                provider.evm_mine(None).await.unwrap();
            }
            .boxed()
        } else {
            future::pending().boxed()
        }
    }

    fn take_kicks(&mut self) -> Option<(Duration, RootProvider)> {
        self.kicks.take()
    }
}

pub trait InfiniteStreamExt: StreamExt + KickExt + Sized + Unpin {
    #[must_use]
    async fn find_map<U>(&mut self, mut f: impl FnMut(Self::Item) -> Option<U>) -> U {
        loop {
            let kick = self.kick();
            tokio::select! {
                _ = kick => {},
                item = self.next() => {
                    let item = item.expect("stream must be infinite");
                    if let Some(res) = f(item) {
                        return res;
                    }
                }
            }
        }
    }

    async fn find(&mut self, mut f: impl FnMut(&Self::Item) -> bool) -> Self::Item {
        self.find_map(|item| if f(&item) { Some(item) } else { None })
            .await
    }
}

impl<T: StreamExt + KickExt + Sized + Unpin> InfiniteStreamExt for T {}

pub fn channel<T>(
    db: Database,
    kicks: Option<(Duration, RootProvider)>,
) -> (EventSender<T>, KickingStream<EventReceiver<T>>) {
    let (mut tx, rx) = async_broadcast::broadcast(1024);
    tx.set_overflow(true);
    let receiver = EventReceiver { inner: rx, db };
    (
        EventSender { inner: tx },
        KickingStream::new(receiver, kicks),
    )
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
    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn new_receiver(&self) -> Self {
        Self {
            inner: self.inner.new_receiver(),
            db: self.db.clone(),
        }
    }
}

impl<T: Clone> KickingStream<EventReceiver<T>> {
    pub fn new_receiver(&self) -> Self {
        Self::new(self.inner.new_receiver(), self.kicks.clone())
    }

    pub fn db(&self) -> &Database {
        self.inner.db()
    }
}

impl TestingEventReceiver {
    #[allow(dead_code)]
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

    /// Drive the compute stream forward until a `BlockPrepared(target)` event
    /// arrives.
    #[allow(dead_code)]
    pub async fn find_block_prepared(&mut self, target: H256) -> H256 {
        self.find_map(|event| match event {
            TestingEvent::Compute(ComputeEvent::BlockPrepared(h)) if h == target => Some(h),
            _ => None,
        })
        .await
    }

    /// Wait until any MB becomes computed, returning its hash.
    #[allow(dead_code)]
    pub async fn find_any_mb_computed(&mut self) -> H256 {
        self.find_map(|event| match event {
            TestingEvent::Compute(ComputeEvent::MbComputed(mb_hash)) => Some(mb_hash),
            _ => None,
        })
        .await
    }

    /// Wait until a finalized MB advances the eth chain to or past
    /// `target_eth_block`. The target need not appear directly in an
    /// `AdvanceTillEthereumBlock` transaction — it suffices that it is an
    /// ancestor of this MB's `last_advanced_eb` (i.e., it sits inside
    /// the eth-chain segment this MB advanced over).
    #[allow(dead_code)]
    pub async fn wait_till_eth_block_finalized_in_mb(&mut self, target_eth_block: H256) {
        self.find_map_with_db(|db, event| {
            let TestingEvent::Malachite(MalachiteEvent::BlockFinalized { mb_hash, .. }) = event
            else {
                return None;
            };
            let last_advanced = db.mb_meta(mb_hash).last_advanced_eb;
            if last_advanced.is_zero() {
                return None;
            }
            // Anchor: previous MB's `last_advanced_eb` (genesis if none).
            let prev_advanced = match db.mb_compact_block(mb_hash) {
                Some(c) if !c.parent.is_zero() => db.mb_meta(c.parent).last_advanced_eb,
                _ => H256::zero(),
            };
            // Walk the eth chain from this MB's `last_advanced_eb` back to
            // the previous anchor; if the target is in that segment, the MB
            // covers it.
            let mut cursor = last_advanced;
            while cursor != prev_advanced {
                if cursor == target_eth_block {
                    return Some(());
                }
                let header = db.block_header(cursor)?;
                if header.parent_hash.is_zero() {
                    break;
                }
                cursor = header.parent_hash;
            }
            None
        })
        .await
    }

    pub async fn find_map_with_db<U>(
        &mut self,
        mut f: impl FnMut(Database, TestingEvent) -> Option<U>,
    ) -> U {
        let db = self.db().clone();
        let func = |event| f(db.clone(), event);
        self.find_map(func).await
    }
}

impl ObserverEventReceiver {
    pub fn filter_map_block(mut self) -> KickingStream<BoxStream<'static, SimpleBlockData>> {
        let kicks = self.take_kicks();
        let stream = self
            .filter_map(|event| async move {
                if let ObserverEvent::Block(block_data) = event {
                    Some(block_data)
                } else {
                    None
                }
            })
            .boxed();
        KickingStream::new(stream, kicks)
    }

    // NOTE: skipped by observer blocks are not iterated (possible on reorgs).
    // If your test depends on events in skipped blocks, you need to improve this method.
    pub fn filter_map_block_synced_with_header(
        mut self,
    ) -> KickingStream<impl Stream<Item = (BlockEvent, SimpleBlockData)>> {
        let db = self.db().clone();
        let kicks = self.take_kicks();
        let stream = self.flat_map(move |event| {
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
        });

        KickingStream::new(stream, kicks)
    }

    pub fn filter_map_block_synced(mut self) -> KickingStream<impl Stream<Item = BlockEvent>> {
        let kicks = self.take_kicks();
        let stream = self
            .filter_map_block_synced_with_header()
            .map(|(event, _)| event);
        KickingStream::new(stream, kicks)
    }
}
