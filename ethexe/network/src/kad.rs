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
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    kad,
    kad::{
        Addresses, EntryView, KBucketKey, PeerRecord, PutRecordError, PutRecordOk, QueryId, Quorum,
        store,
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
    task::{Context, Poll, ready},
    time::Duration,
};

const KAD_RECORD_TTL_SECS: u64 = 3600 * 3; // 3 hours
const KAD_RECORD_TTL: Duration = Duration::from_secs(KAD_RECORD_TTL_SECS);
const KAD_PUBLISHING_INTERVAL: Duration = Duration::from_secs(KAD_RECORD_TTL_SECS / 4);
// From Substrate sources:
// This number is small enough to make sure we don't
// unnecessarily flood the network with queries, but high
// enough to make sure we also touch peers which might have
// old record, so that we can update them once we notice
// they have old records.
const MIN_QUORUM_PEERS: u32 = 4;

#[derive(Debug, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorIdentityKey {
    pub validator: Address,
}

#[derive(Debug, PartialEq, Eq)]
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

#[derive(Debug, PartialEq, Eq, Encode, Decode, derive_more::From)]
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

#[derive(Debug, PartialEq, Eq, derive_more::From)]
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

    fn into_kad_record(self) -> kad::Record {
        match self {
            Record::ValidatorIdentity(record) => {
                let key = record.key();
                let ValidatorIdentityRecord { value } = record;
                kad::Record::new(RecordKey::ValidatorIdentity(key).encode(), value.encode())
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
pub struct GetRecordOk {
    pub peer: Option<PeerId>,
    pub record: Record,
}

#[derive(Debug, PartialEq, Eq, derive_more::Display)]
pub enum GetRecordError {
    #[display("Record not found: key={key:?}")]
    NotFound { key: RecordKey },
}

#[derive(Debug)]
pub enum Event {
    RoutingUpdated {
        peer: PeerId,
    },
    GetRecord(Result<Box<GetRecordOk>, GetRecordError>),
    PutRecord(Result<RecordKey, PutRecordError>),
    InboundPutRecord {
        // might be used in the future
        #[allow(unused)]
        source: PeerId,
        validator: Box<PutRecordValidator>,
    },
}

pub struct Behaviour {
    inner: kad::Behaviour<MemoryStore>,
    peer_score: peer_score::Handle,
    cache_candidates_records: HashMap<QueryId, kad::Record>,
}

impl Behaviour {
    pub fn new(peer: PeerId, peer_score: peer_score::Handle) -> Self {
        let mut inner = kad::Config::default();
        inner
            .disjoint_query_paths(true)
            .set_record_ttl(Some(KAD_RECORD_TTL))
            .set_publication_interval(Some(KAD_PUBLISHING_INTERVAL))
            .set_record_filtering(kad::StoreInserts::FilterBoth);
        let mut inner = kad::Behaviour::with_config(peer, MemoryStore::new(peer), inner);
        inner.set_mode(Some(kad::Mode::Server));
        Self {
            inner,
            peer_score,
            cache_candidates_records: HashMap::new(),
        }
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

    pub fn get_record(&mut self, key: impl Into<RecordKey>) -> QueryId {
        self.inner.get_record(key.into().into_kad_key())
    }

    pub fn put_record(&mut self, record: impl Into<Record>) -> Result<QueryId, store::Error> {
        self.inner
            .put_record(record.into().into_kad_record(), Quorum::All)
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
                    let result = match result {
                        Ok(kad::GetRecordOk::FoundRecord(PeerRecord {
                            peer,
                            record: original_record,
                        })) => {
                            if stats.num_successes() > MIN_QUORUM_PEERS
                                && let Some(mut query) = self.inner.query_mut(&id)
                            {
                                query.finish();
                            }

                            let record = match Record::new(&original_record) {
                                Ok(record) => record,
                                Err(err) => {
                                    log::trace!("failed to get record: {err}");
                                    if let Some(peer) = peer {
                                        // NOTE: not backward compatible if `Record` have new variant, and it is decoded by the old node
                                        self.peer_score.invalid_data(peer);
                                    } else {
                                        #[cfg(debug_assertions)]
                                        unreachable!("local storage is corrupted");
                                    }
                                    return Poll::Pending;
                                }
                            };

                            self.cache_candidates_records.insert(id, original_record);

                            Ok(Box::new(GetRecordOk { peer, record }))
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

                            return Poll::Pending;
                        }
                        Err(kad::GetRecordError::NotFound {
                            key,
                            closest_peers: _,
                        }) => {
                            let key = RecordKey::new(&key)
                                .expect("invalid record key that we got from local storage");
                            Err(GetRecordError::NotFound { key })
                        }
                        Err(err) => {
                            log::trace!("failed to get record: {err}");
                            return Poll::Pending;
                        }
                    };
                    return Poll::Ready(Event::GetRecord(result));
                }
                kad::QueryResult::PutRecord(result) => {
                    let result = match result {
                        Ok(PutRecordOk { key }) => {
                            let key = RecordKey::new(&key)
                                // we are the ones who called `Kad::put_record` and thus the key must be decoded without issues
                                .expect("invalid record key that we put ourselves");
                            Ok(key)
                        }
                        Err(err) => Err(err),
                    };
                    return Poll::Ready(Event::PutRecord(result));
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
