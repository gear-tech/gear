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

use crate::{peer_score, validator::discovery::SignedValidatorIdentity};
use anyhow::Context as _;
use ethexe_common::Address;
use futures::{FutureExt, Stream, stream::FusedStream};
use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    core::{Endpoint, transport::PortUse},
    kad,
    kad::{
        Addresses, EntryView, KBucketKey, PeerRecord, PutRecordOk, QueryId, Quorum, store,
        store::{MemoryStore, RecordStore},
    },
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::HashMap,
    pin::Pin,
    task::{Context, Poll, ready},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const KAD_PROTOCOL_NAME: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/kad/", env!("CARGO_PKG_VERSION")));
const KAD_RECORD_TTL_SECS: u64 = 3600 * 3; // 3 hours
const KAD_RECORD_TTL: Duration = Duration::from_secs(KAD_RECORD_TTL_SECS);
const KAD_PUBLISHING_INTERVAL: Duration = Duration::from_secs(KAD_RECORD_TTL_SECS / 4);
// From Substrate sources:
// This number is small enough to make sure we don't
// unnecessarily flood the network with queries, but high
// enough to make sure we also touch peers which might have
// old record, so that we can update them once we notice
// they have old records.
const KAD_MIN_QUORUM_PEERS: u32 = 4;

#[derive(Debug, PartialEq, Eq, Encode, Decode, Clone)]
pub struct ValidatorIdentityKey {
    pub validator: Address,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ValidatorIdentityRecord {
    pub value: SignedValidatorIdentity,
}

impl ValidatorIdentityRecord {
    pub fn key(&self) -> ValidatorIdentityKey {
        ValidatorIdentityKey {
            validator: self.value.address(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Encode, Decode, derive_more::From, derive_more::Unwrap, Clone)]
pub enum RecordKey {
    ValidatorIdentity(ValidatorIdentityKey),
}

impl RecordKey {
    fn new(key: &kad::RecordKey) -> Result<Self, parity_scale_codec::Error> {
        Decode::decode(&mut &key.as_ref()[..])
    }

    fn into_kad_key(self) -> kad::RecordKey {
        kad::RecordKey::new(&self.encode())
    }
}

#[derive(Debug, PartialEq, Eq, derive_more::From, derive_more::Unwrap, Clone)]
pub enum Record {
    ValidatorIdentity(ValidatorIdentityRecord),
}

impl Record {
    fn new(record: &kad::Record) -> anyhow::Result<Self> {
        let key = RecordKey::new(&record.key)?;
        match key {
            RecordKey::ValidatorIdentity(key) => {
                let value: SignedValidatorIdentity = Decode::decode(&mut &record.value[..])
                    .context("failed to decode validator identity")?;

                let ValidatorIdentityKey { validator } = key;
                anyhow::ensure!(
                    validator == value.address(),
                    "validator address of record key mismatches address of record value"
                );

                Ok(Self::ValidatorIdentity(ValidatorIdentityRecord { value }))
            }
        }
    }

    fn key(&self) -> RecordKey {
        match self {
            Record::ValidatorIdentity(record) => RecordKey::ValidatorIdentity(record.key()),
        }
    }

    fn into_kad_record(self) -> kad::Record {
        let key = self.key();
        match self {
            Record::ValidatorIdentity(record) => {
                let ValidatorIdentityRecord { value } = record;
                kad::Record::new(key.encode(), value.encode())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PutRecordValidator {
    original_record: kad::Record,
    record: Record,
}

impl PutRecordValidator {
    pub fn validate<F>(self, behaviour: &mut Behaviour, f: F)
    where
        F: FnOnce(Record) -> bool,
    {
        let Self {
            original_record,
            record,
        } = self;
        let success = f(record);
        if success {
            let _res = behaviour.inner.store_mut().put(original_record);
        }
    }
}

#[derive(Debug)]
pub enum Event {
    RoutingUpdated {
        peer: PeerId,
    },
    GetRecordStarted {
        #[allow(unused)] // used in tests
        query_id: QueryId,
    },
    PutRecordStarted {
        #[allow(unused)] // used in tests
        query_id: QueryId,
    },
    InboundPutRecord {
        // might be used in the future
        #[allow(unused)]
        source: PeerId,
        validator: Box<PutRecordValidator>,
    },
}

pub type GetRecordResult = Result<GetRecordOk, GetRecordError>;

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GetRecordOk {
    pub peer: Option<PeerId>,
    pub record: Record,
}

#[derive(Debug, PartialEq, Eq, derive_more::Display, Clone)]
pub enum GetRecordError {
    #[display("Record not found: key={key:?}")]
    NotFound { key: RecordKey },
}

pub struct GetRecordFuture {
    inner: mpsc::UnboundedReceiver<GetRecordResult>,
}

impl Stream for GetRecordFuture {
    type Item = GetRecordResult;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_recv(cx)
    }
}

impl FusedStream for GetRecordFuture {
    fn is_terminated(&self) -> bool {
        self.inner.is_closed()
    }
}

pub type PutRecordResult = Result<RecordKey, PutRecordError>;

#[derive(Debug, derive_more::Display)]
pub enum PutRecordError {
    #[display("KAD error: {_0}")]
    Kad(kad::PutRecordError),
    #[display("failed to store record: {_0}")]
    Store(store::Error),
}

pub struct PutRecordFuture {
    inner: oneshot::Receiver<PutRecordResult>,
}

impl Future for PutRecordFuture {
    type Output = PutRecordResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.inner
            .poll_unpin(cx)
            .map(|res| res.expect("channel should never be closed"))
    }
}

#[derive(Debug)]
enum HandlerAction {
    GetRecord(RecordKey, mpsc::UnboundedSender<GetRecordResult>),
    PutRecord(Box<Record>, oneshot::Sender<PutRecordResult>),
}

#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<HandlerAction>);

impl Handle {
    pub fn get_record(&self, key: RecordKey) -> GetRecordFuture {
        let (tx, rx) = mpsc::unbounded_channel();
        self.0
            .send(HandlerAction::GetRecord(key, tx))
            .expect("channel should never be closed");
        GetRecordFuture { inner: rx }
    }

    pub fn put_record(&self, record: Record) -> PutRecordFuture {
        let (tx, rx) = oneshot::channel();
        self.0
            .send(HandlerAction::PutRecord(Box::new(record), tx))
            .expect("channel should never be closed");
        PutRecordFuture { inner: rx }
    }

    #[cfg(test)]
    pub fn new_test() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        Handle(tx)
    }
}

pub struct Behaviour {
    inner: kad::Behaviour<MemoryStore>,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<HandlerAction>,
    get_record_queries: HashMap<QueryId, mpsc::UnboundedSender<GetRecordResult>>,
    put_record_queries: HashMap<QueryId, oneshot::Sender<PutRecordResult>>,
    peer_score: peer_score::Handle,
    cache_candidates_records: HashMap<QueryId, kad::Record>,
    min_quorum_peers: u32,
    #[cfg(test)]
    early_finished_queries: std::collections::HashSet<QueryId>,
}

impl Behaviour {
    pub fn new(peer: PeerId, peer_score: peer_score::Handle) -> Self {
        Self::with_min_quorum(peer, peer_score, KAD_MIN_QUORUM_PEERS)
    }

    fn with_min_quorum(
        peer: PeerId,
        peer_score: peer_score::Handle,
        min_quorum_peers: u32,
    ) -> Self {
        let mut inner = kad::Config::new(KAD_PROTOCOL_NAME);
        inner
            .disjoint_query_paths(true)
            .set_record_ttl(Some(KAD_RECORD_TTL))
            .set_publication_interval(Some(KAD_PUBLISHING_INTERVAL))
            .set_record_filtering(kad::StoreInserts::FilterBoth)
            // only mDNS, bootstrap and directly connected peers will be inserted into the routing table
            .set_kbucket_inserts(kad::BucketInserts::Manual);
        let mut inner = kad::Behaviour::with_config(peer, MemoryStore::new(peer), inner);
        inner.set_mode(Some(kad::Mode::Server));

        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Handle(tx);

        Self {
            inner,
            handle,
            rx,
            get_record_queries: HashMap::new(),
            put_record_queries: HashMap::new(),
            peer_score,
            cache_candidates_records: HashMap::new(),
            min_quorum_peers,
            #[cfg(test)]
            early_finished_queries: std::collections::HashSet::new(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    pub fn add_address(&mut self, peer_id: PeerId, multiaddr: Multiaddr) {
        self.inner.add_address(&peer_id, multiaddr);
    }

    pub fn remove_peer(
        &mut self,
        peer_id: PeerId,
    ) -> Option<EntryView<KBucketKey<PeerId>, Addresses>> {
        self.inner.remove_peer(&peer_id)
    }

    fn handle_inner_event(&mut self, event: kad::Event) -> Poll<Event> {
        match event {
            kad::Event::RoutingUpdated { peer, .. } => {
                return Poll::Ready(Event::RoutingUpdated { peer });
            }
            kad::Event::InboundRequest {
                request:
                    kad::InboundRequest::PutRecord {
                        source,
                        connection: _,
                        record,
                    },
            } => {
                let original_record =
                    record.expect("`StoreInserts::FilterBoth` implies `record` is always present");
                let record = match Record::new(&original_record) {
                    Ok(record) => record,
                    Err(_err) => {
                        // TODO: peer score
                        return Poll::Pending;
                    }
                };
                let validator = PutRecordValidator {
                    original_record,
                    record,
                };
                return Poll::Ready(Event::InboundPutRecord {
                    source,
                    validator: Box::new(validator),
                });
            }
            kad::Event::OutboundQueryProgressed {
                id,
                result,
                stats,
                step: _,
            } => match result {
                kad::QueryResult::GetRecord(result) => {
                    match result {
                        Ok(kad::GetRecordOk::FoundRecord(PeerRecord {
                            peer,
                            record: original_record,
                        })) => {
                            if stats.num_successes() >= self.min_quorum_peers
                                && let Some(mut query) = self.inner.query_mut(&id)
                            {
                                #[cfg(test)]
                                self.early_finished_queries.insert(query.id());

                                query.finish();
                            }

                            let record = match Record::new(&original_record) {
                                Ok(record) => record,
                                Err(err) => {
                                    log::trace!("failed to get record: {err}");
                                    if let Some(peer) = peer {
                                        // NOTE: not backward compatible if `Record` has a new variant, and it is decoded by the old node
                                        self.peer_score.invalid_data(peer);
                                    } else {
                                        #[cfg(debug_assertions)]
                                        unreachable!("local storage is corrupted");
                                    }
                                    return Poll::Pending;
                                }
                            };

                            let channel = self.get_record_queries.get(&id).expect("unknown query");
                            let result = Ok(GetRecordOk { peer, record });
                            let _res = channel.send(result);

                            // TODO: we might overwrite existing record so possibly we need to check if it newer
                            self.cache_candidates_records.insert(id, original_record);
                        }
                        Ok(kad::GetRecordOk::FinishedWithNoAdditionalRecord {
                            cache_candidates,
                        }) => {
                            if let Some(record) = self.cache_candidates_records.remove(&id)
                                // `put_record_to` just fails if there are no peers
                                && !cache_candidates.is_empty()
                            {
                                self.inner.put_record_to(
                                    record,
                                    cache_candidates.into_values(),
                                    Quorum::One,
                                );
                            }

                            // drop the channel so other side will receive `None`
                            let _channel =
                                self.get_record_queries.remove(&id).expect("unknown query");
                        }
                        Err(kad::GetRecordError::NotFound {
                            key,
                            closest_peers: _,
                        }) => {
                            let key = RecordKey::new(&key)
                                .expect("invalid record key that we got from local storage");

                            let err = GetRecordError::NotFound { key };

                            let channel =
                                self.get_record_queries.remove(&id).expect("unknown query");
                            let _res = channel.send(Err(err));
                        }
                        Err(err) => {
                            log::trace!("failed to get record: {err}");
                        }
                    }
                }
                kad::QueryResult::PutRecord(result) => {
                    let result = match result {
                        Ok(PutRecordOk { key }) => {
                            let key = RecordKey::new(&key)
                                // we are the ones who called `Kad::put_record` and thus the key must be decoded without issues
                                .expect("invalid record key that we put ourselves");
                            Ok(key)
                        }
                        Err(err) => Err(PutRecordError::Kad(err)),
                    };

                    // we make `put_record_to` so map may not contain query
                    if let Some(channel) = self.put_record_queries.remove(&id) {
                        let _res = channel.send(result);
                    }
                }
                _ => {}
            },
            _ => {}
        }

        Poll::Pending
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<kad::Behaviour<MemoryStore>>;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.inner.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(Some(action)) = self.rx.poll_recv(cx) {
            match action {
                HandlerAction::GetRecord(record_key, channel) => {
                    let query_id = self.inner.get_record(record_key.into_kad_key());
                    self.get_record_queries.insert(query_id, channel);
                    return Poll::Ready(ToSwarm::GenerateEvent(Event::GetRecordStarted {
                        query_id,
                    }));
                }
                HandlerAction::PutRecord(record, channel) => {
                    match self.inner.put_record(record.into_kad_record(), Quorum::All) {
                        Ok(query_id) => {
                            self.put_record_queries.insert(query_id, channel);
                            return Poll::Ready(ToSwarm::GenerateEvent(Event::PutRecordStarted {
                                query_id,
                            }));
                        }
                        Err(err) => {
                            let _res = channel.send(Err(PutRecordError::Store(err)));
                        }
                    }
                }
            }
        }

        let to_swarm = ready!(self.inner.poll(cx));
        match to_swarm {
            ToSwarm::GenerateEvent(event) => {
                self.handle_inner_event(event).map(ToSwarm::GenerateEvent)
            }
            to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                unreachable!("`ToSwarm::GenerateEvent` is handled above")
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        utils::tests::init_logger,
        validator::discovery::{ValidatorAddresses, ValidatorIdentity},
    };
    use assert_matches::assert_matches;
    use ethexe_signer::Signer;
    use futures::StreamExt;
    use libp2p::{
        Swarm, identity::Keypair, kad, kad::GetRecordOk as KadGetRecordOk, swarm::ConnectionId,
    };
    use libp2p_swarm_test::SwarmExt;
    use std::{collections::BTreeMap, num::NonZeroUsize};

    fn new_identity() -> SignedValidatorIdentity {
        let keypair = Keypair::generate_secp256k1();
        let signer = Signer::memory();
        let validator_key = signer.generate_key().unwrap();
        let identity = ValidatorIdentity {
            addresses: ValidatorAddresses::new(
                keypair.public().to_peer_id(),
                "/ip4/127.0.0.1/tcp/30333".parse().unwrap(),
            ),
            creation_time: 1,
        };
        identity
            .sign(&signer, validator_key, &keypair)
            .expect("signing validator identity should work")
    }

    fn new_behaviour() -> Behaviour {
        new_behaviour_with_quorum(KAD_MIN_QUORUM_PEERS)
    }

    fn new_behaviour_with_quorum(min_quorum_peers: u32) -> Behaviour {
        let peer_id = Keypair::generate_ed25519().public().to_peer_id();
        Behaviour::with_min_quorum(peer_id, peer_score::Handle::new_test(), min_quorum_peers)
    }

    async fn new_swarm() -> Swarm<Behaviour> {
        new_swarm_with_quorum(KAD_MIN_QUORUM_PEERS).await
    }

    async fn new_swarm_with_quorum(min_quorum_peers: u32) -> Swarm<Behaviour> {
        let mut swarm = Swarm::new_ephemeral_tokio(move |keypair| {
            let peer_id = keypair.public().to_peer_id();
            Behaviour::with_min_quorum(peer_id, peer_score::Handle::new_test(), min_quorum_peers)
        });
        swarm.listen().with_memory_addr_external().await;
        swarm
    }

    fn add_bootstrap_addresses<const N: usize>(swarms: [&mut Swarm<Behaviour>; N]) {
        let peers: Vec<_> = swarms
            .iter()
            .map(|swarm| {
                (
                    *swarm.local_peer_id(),
                    swarm.external_addresses().next().unwrap().clone(),
                )
            })
            .collect();

        for swarm in swarms {
            for (peer_id, addr) in peers.clone() {
                if peer_id == *swarm.local_peer_id() {
                    continue;
                }

                swarm.behaviour_mut().add_address(peer_id, addr);
            }
        }
    }

    async fn start_query(
        swarm: &mut Swarm<Behaviour>,
        key: RecordKey,
    ) -> (QueryId, GetRecordFuture) {
        let fut = swarm.behaviour().handle().get_record(key.clone());

        let event = swarm.next_behaviour_event().await;
        let Event::GetRecordStarted { query_id } = event else {
            unreachable!("Unexpected event: {event:?}")
        };

        (query_id, fut)
    }

    fn store_identity(behaviour: &mut Behaviour, signed: SignedValidatorIdentity) {
        let record =
            Record::ValidatorIdentity(ValidatorIdentityRecord { value: signed }).into_kad_record();
        behaviour.inner.store_mut().put(record).unwrap();
    }

    #[test]
    fn record_encode_decode() {
        let signed = new_identity();
        let kad_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();

        let record = Record::new(&kad_record)
            .expect("record must decode")
            .unwrap_validator_identity();
        assert_eq!(record.value, signed);
    }

    #[test]
    fn record_errors_on_mismatched_validator() {
        let signed = new_identity();
        let mismatched_key = ValidatorIdentityKey {
            validator: Address::from(42u64),
        };
        let kad_record = kad::Record::new(
            RecordKey::ValidatorIdentity(mismatched_key).encode(),
            signed.encode(),
        );

        Record::new(&kad_record).unwrap_err();
    }

    #[test]
    fn validator_stores_record_after_successful_check() {
        let signed = new_identity();
        let mut behaviour = new_behaviour();
        let original_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();
        let key = original_record.key.clone();
        let validator = PutRecordValidator {
            original_record,
            record: Record::ValidatorIdentity(ValidatorIdentityRecord { value: signed }),
        };

        validator.validate(&mut behaviour, |_record| true);

        assert!(behaviour.inner.store_mut().get(&key).is_some());
    }

    #[test]
    fn validator_does_not_store_when_check_fails() {
        let signed = new_identity();
        let mut behaviour = new_behaviour();
        let original_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();
        let key = original_record.key.clone();
        let validator = PutRecordValidator {
            original_record,
            record: Record::ValidatorIdentity(ValidatorIdentityRecord { value: signed }),
        };

        validator.validate(&mut behaviour, |_record| false);

        assert!(behaviour.inner.store_mut().get(&key).is_none());
    }

    #[test]
    fn inbound_put_record_emits_event_with_validator() {
        let signed = new_identity();
        let mut behaviour = new_behaviour();
        let peer = PeerId::random();
        let kad_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();
        let event = kad::Event::InboundRequest {
            request: kad::InboundRequest::PutRecord {
                source: peer,
                connection: ConnectionId::new_unchecked(1),
                record: Some(kad_record),
            },
        };

        let Poll::Ready(Event::InboundPutRecord { source, validator }) =
            behaviour.handle_inner_event(event)
        else {
            panic!("poll is pending")
        };

        assert_eq!(source, peer);
        let PutRecordValidator { record, .. } = *validator;
        let record = record.unwrap_validator_identity();
        assert_eq!(record.value, signed);
    }

    #[tokio::test]
    async fn get_record_success_is_reported_and_cached() {
        let signed = new_identity();
        let mut swarm = new_swarm().await;
        let (query_id, mut fut) = start_query(
            &mut swarm,
            RecordKey::ValidatorIdentity(ValidatorIdentityKey {
                validator: signed.address(),
            }),
        )
        .await;
        let peer = PeerId::random();
        let kad_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();
        let step = kad::ProgressStep {
            count: NonZeroUsize::new(1).unwrap(),
            last: true,
        };
        let event = kad::Event::OutboundQueryProgressed {
            id: query_id,
            result: kad::QueryResult::GetRecord(Ok(KadGetRecordOk::FoundRecord(PeerRecord {
                peer: Some(peer),
                record: kad_record.clone(),
            }))),
            stats: kad::QueryStats::empty(),
            step,
        };

        let _ = swarm.behaviour_mut().handle_inner_event(event);
        let result = fut.next().await.unwrap().unwrap();
        assert_eq!(result.peer, Some(peer));
        assert_eq!(result.record.unwrap_validator_identity().value, signed);

        assert_eq!(
            swarm
                .behaviour_mut()
                .cache_candidates_records
                .get(&query_id)
                .map(|rec| rec.value.clone()),
            Some(kad_record.value)
        );
    }

    #[tokio::test]
    async fn finished_without_additional_record_removes_cached_entry() {
        let signed = new_identity();
        let mut swarm = new_swarm().await;
        let validator = signed.address();
        let (query_id, _fut) = start_query(
            &mut swarm,
            RecordKey::ValidatorIdentity(ValidatorIdentityKey { validator }),
        )
        .await;
        let cached_record = Record::ValidatorIdentity(ValidatorIdentityRecord {
            value: signed.clone(),
        })
        .into_kad_record();
        swarm
            .behaviour_mut()
            .cache_candidates_records
            .insert(query_id, cached_record);

        let cache_candidates = BTreeMap::new();
        let step = kad::ProgressStep {
            count: NonZeroUsize::new(1).unwrap(),
            last: true,
        };
        let event = kad::Event::OutboundQueryProgressed {
            id: query_id,
            result: kad::QueryResult::GetRecord(Ok(
                KadGetRecordOk::FinishedWithNoAdditionalRecord { cache_candidates },
            )),
            stats: kad::QueryStats::empty(),
            step,
        };

        assert_matches!(
            swarm.behaviour_mut().handle_inner_event(event),
            Poll::Pending
        );
        assert!(swarm.behaviour_mut().cache_candidates_records.is_empty());
    }

    #[tokio::test]
    async fn get_record_not_found_propagates_error() {
        let signed = new_identity();
        let mut swarm = new_swarm().await;
        let validator = signed.address();
        let (query_id, mut fut) = start_query(
            &mut swarm,
            RecordKey::ValidatorIdentity(ValidatorIdentityKey { validator }),
        )
        .await;
        let kad_key =
            RecordKey::ValidatorIdentity(ValidatorIdentityKey { validator }).into_kad_key();
        let step = kad::ProgressStep {
            count: NonZeroUsize::new(1).unwrap(),
            last: true,
        };
        let event = kad::Event::OutboundQueryProgressed {
            id: query_id,
            result: kad::QueryResult::GetRecord(Err(kad::GetRecordError::NotFound {
                key: kad_key,
                closest_peers: Vec::new(),
            })),
            stats: kad::QueryStats::empty(),
            step,
        };

        let _ = swarm.behaviour_mut().handle_inner_event(event);
        let Err(GetRecordError::NotFound { key }) = fut.next().await.unwrap() else {
            panic!("expected not found")
        };
        let ValidatorIdentityKey { validator: got } = key.unwrap_validator_identity();
        assert_eq!(got, validator);
    }

    #[tokio::test]
    async fn query_finishes_once_quorum_reached() {
        const MIN_QUORUM: u32 = 1;

        init_logger();

        let signed = new_identity();
        let mut alice = new_swarm_with_quorum(MIN_QUORUM).await;
        let mut bob = new_swarm_with_quorum(MIN_QUORUM).await;
        let mut charlie = new_swarm_with_quorum(MIN_QUORUM).await;
        alice.connect(&mut bob).await;
        alice.connect(&mut charlie).await;
        add_bootstrap_addresses([&mut alice, &mut bob, &mut charlie]);

        store_identity(bob.behaviour_mut(), signed.clone());
        tokio::spawn(bob.loop_on_next());
        store_identity(charlie.behaviour_mut(), signed.clone());
        tokio::spawn(charlie.loop_on_next());

        let key = RecordKey::ValidatorIdentity(ValidatorIdentityKey {
            validator: signed.address(),
        });
        let (query_id, mut fut) = start_query(&mut alice, key).await;

        // skip events for Bob and Charlie
        for _ in 0..2 {
            let event = alice.next_behaviour_event().await;
            assert_matches!(event, Event::RoutingUpdated { .. });
        }

        let record = loop {
            tokio::select! {
                res = fut.next() => break res.unwrap().unwrap().record.unwrap_validator_identity(),
                _ = alice.next_behaviour_event() => {},
            }
        };
        assert_eq!(record.value, signed);

        // at this moment `inner` has not yet incremented succeeded requests counter
        let record = loop {
            tokio::select! {
                res = fut.next() => break res.unwrap().unwrap().record.unwrap_validator_identity(),
                _ = alice.next_behaviour_event() => {},
            }
        };
        assert_eq!(record.value, signed);

        assert!(alice.behaviour().early_finished_queries.contains(&query_id));
    }

    #[tokio::test]
    async fn query_stays_active_when_quorum_not_met() {
        const MIN_QUORUM: u32 = 100;

        init_logger();

        let signed = new_identity();
        let mut alice = new_swarm_with_quorum(MIN_QUORUM).await;
        let mut bob = new_swarm_with_quorum(MIN_QUORUM).await;
        alice.connect(&mut bob).await;
        add_bootstrap_addresses([&mut alice, &mut bob]);

        store_identity(bob.behaviour_mut(), signed.clone());
        tokio::spawn(bob.loop_on_next());

        let key = RecordKey::ValidatorIdentity(ValidatorIdentityKey {
            validator: signed.address(),
        });
        let (query_id, mut fut) = start_query(&mut alice, key).await;

        let event = alice.next_behaviour_event().await;
        assert_matches!(event, Event::RoutingUpdated { .. });

        let record = loop {
            tokio::select! {
                res = fut.next() => break res.unwrap().unwrap().record.unwrap_validator_identity(),
                _ = alice.next_behaviour_event() => {},
            }
        };
        assert_eq!(record.value, signed);

        assert!(!alice.behaviour().early_finished_queries.contains(&query_id));
    }
}
