// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{
    export::{Multiaddr, PeerId},
    utils::ParityScaleCodec,
};
use ethexe_db::{BlockMetaStorage, CodesStorage, Database};
use gear_core::ids::ProgramId;
use gprimitives::{ActorId, CodeId, H256};
use libp2p::{
    core::Endpoint,
    futures::FutureExt,
    request_response,
    request_response::{
        InboundFailure, Message, OutboundFailure, OutboundRequestId, ProtocolSupport,
    },
    swarm::{
        behaviour::ConnectionEstablished, CloseConnection, ConnectionClosed, ConnectionDenied,
        ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent,
        ToSwarm,
    },
    StreamProtocol,
};
use parity_scale_codec::{Decode, Encode};
use rand::seq::IteratorRandom;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    task::{Context, Poll},
};
use tokio::task::JoinHandle;

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

#[derive(Debug, Eq, PartialEq)]
pub enum RequestFailure {
    /// No available peers to complete request
    NoAvailablePeers,
    /// Request kind unequal to response kind
    TypeMismatch,
    /// Hash field in request unequal to one in response
    HashInequality,
    /// Response contains more data than requested
    ExcessiveData,
    /// Hashed data unequal to its corresponding hash
    DataHashMismatch,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct RequestId(u64);

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode)]
pub enum Request {
    BlockEndProgramStates(H256),
    DataForHashes(BTreeSet<H256>),
    ProgramCodeIds(BTreeSet<ProgramId>),
}

impl Request {
    fn validate_response(&self, resp: &Response) -> Result<(), RequestFailure> {
        match (self, resp) {
            (
                Request::BlockEndProgramStates(requested_block_hash),
                Response::BlockEndProgramStates {
                    block_hash,
                    states: _,
                },
            ) => {
                if requested_block_hash == block_hash {
                    Ok(())
                } else {
                    Err(RequestFailure::HashInequality)
                }
            }
            (Request::DataForHashes(requested_hashes), Response::DataForHashes(hashes)) => {
                for (hash, data) in hashes {
                    if !requested_hashes.contains(hash) {
                        return Err(RequestFailure::ExcessiveData);
                    }

                    if *hash != ethexe_db::hash(data) {
                        return Err(RequestFailure::DataHashMismatch);
                    }
                }

                Ok(())
            }
            (Request::ProgramCodeIds(requested_ids), Response::ProgramCodeIds(ids)) => {
                for pid in ids.keys() {
                    if !requested_ids.contains(pid) {
                        return Err(RequestFailure::ExcessiveData);
                    }
                }

                Ok(())
            }
            (_, _) => Err(RequestFailure::TypeMismatch),
        }
    }

    /// Calculate missing request keys in response and create a new request with these keys
    fn difference(&self, resp: &Response) -> Option<Self> {
        match (self, resp) {
            (
                Request::BlockEndProgramStates(_request_block_hash),
                Response::BlockEndProgramStates { .. },
            ) => None,
            (Request::DataForHashes(requested_hashes), Response::DataForHashes(hashes)) => {
                let hashes_keys = hashes.keys().copied().collect();
                let new_requested_hashes: BTreeSet<H256> =
                    requested_hashes.difference(&hashes_keys).copied().collect();
                if !new_requested_hashes.is_empty() {
                    Some(Request::DataForHashes(new_requested_hashes))
                } else {
                    None
                }
            }
            (Request::ProgramCodeIds(requested_ids), Response::ProgramCodeIds(ids)) => {
                let ids_keys = ids.keys().copied().collect();
                let new_requested_ids: BTreeSet<ProgramId> =
                    requested_ids.difference(&ids_keys).copied().collect();
                if !new_requested_ids.is_empty() {
                    Some(Request::ProgramCodeIds(new_requested_ids))
                } else {
                    None
                }
            }
            _ => unreachable!("should be checked in `validate_response`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Encode, Decode)]
pub enum Response {
    BlockEndProgramStates {
        /// Block hash states requested for
        block_hash: H256,
        /// Program states for request block
        states: BTreeMap<ActorId, H256>,
    },
    /// Key (hash) - value (bytes) data
    DataForHashes(BTreeMap<H256, Vec<u8>>),
    /// Program IDs and their corresponding code IDs
    ProgramCodeIds(BTreeMap<ProgramId, CodeId>),
}

impl Response {
    fn merge(self, new_response: Response) -> Response {
        match (self, new_response) {
            (
                Response::BlockEndProgramStates {
                    block_hash,
                    mut states,
                },
                Response::BlockEndProgramStates {
                    states: new_states, ..
                },
            ) => {
                states.extend(new_states);
                Response::BlockEndProgramStates { block_hash, states }
            }
            (Response::DataForHashes(mut data), Response::DataForHashes(new_data)) => {
                data.extend(new_data);
                Response::DataForHashes(data)
            }
            (Response::ProgramCodeIds(mut ids), Response::ProgramCodeIds(new_ids)) => {
                ids.extend(new_ids);
                Response::ProgramCodeIds(ids)
            }
            _ => unreachable!("should be checked in `validate_response`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
#[allow(clippy::enum_variant_names)]
pub enum Event {
    NewRequestRound {
        /// The ID of request
        request_id: RequestId,
        /// Peer we are currently requesting to
        peer_id: PeerId,
    },
    RequestSucceed {
        /// The ID of request
        request_id: RequestId,
        /// Response itself
        response: Response,
    },
    RequestFailed {
        /// The ID of request
        request_id: RequestId,
        /// Reason of request failure
        error: RequestFailure,
    },
}

enum OngoingRequestCompletion {
    Full(Response),
    Partial(OngoingRequest),
}

#[derive(Debug)]
struct OngoingRequest {
    request_id: RequestId,
    request: Request,
    response: Option<Response>,
    tried_peers: HashSet<PeerId>,
}

impl OngoingRequest {
    fn new(request_id: RequestId, request: Request) -> Self {
        Self {
            request_id,
            request,
            response: None,
            tried_peers: HashSet::new(),
        }
    }

    fn merge_response(&mut self, new_response: Response) -> Response {
        if let Some(response) = self.response.take() {
            response.merge(new_response)
        } else {
            new_response
        }
    }

    /// Try to bring request to the complete state.
    ///
    /// Returns error if response validation is failed.
    fn try_complete(
        mut self,
        peer: PeerId,
        response: Response,
    ) -> Result<OngoingRequestCompletion, RequestFailure> {
        self.request.validate_response(&response)?;

        if let Some(new_request) = self.request.difference(&response) {
            self.request = new_request;
            self.tried_peers.insert(peer);
            self.response = Some(self.merge_response(response));
            Ok(OngoingRequestCompletion::Partial(self))
        } else {
            let response = self.merge_response(response);
            Ok(OngoingRequestCompletion::Full(response))
        }
    }

    fn peer_failed(mut self, peer: PeerId) -> Self {
        self.tried_peers.insert(peer);
        self
    }

    fn choose_next_peer(
        &mut self,
        peers: &HashMap<PeerId, HashSet<ConnectionId>>,
    ) -> Option<PeerId> {
        let peers: HashSet<PeerId> = peers.keys().copied().collect();
        let peer = peers
            .difference(&self.tried_peers)
            .choose_stable(&mut rand::thread_rng())
            .copied();
        peer
    }
}

#[derive(Debug, Default)]
struct OngoingRequests {
    inner: HashMap<OutboundRequestId, OngoingRequest>,
    connections: HashMap<PeerId, HashSet<ConnectionId>>,
}

impl OngoingRequests {
    /// Tracks all active connections.
    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionEstablished(ConnectionEstablished {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connections
                    .entry(peer_id)
                    .or_default()
                    .insert(connection_id);
            }
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                ..
            }) => {
                self.connections
                    .entry(peer_id)
                    .or_default()
                    .remove(&connection_id);
            }
            _ => {}
        }
    }

    fn remove(&mut self, outbound_request_id: OutboundRequestId) -> Option<OngoingRequest> {
        self.inner.remove(&outbound_request_id)
    }

    /// Send actual request to behaviour and tracks its state.
    ///
    /// On success, returns peer ID we sent request to.
    ///
    /// On error, returns request back if no peer connected to the swarm.
    fn send_request(
        &mut self,
        behaviour: &mut InnerBehaviour,
        mut ongoing_request: OngoingRequest,
    ) -> Result<PeerId, OngoingRequest> {
        let peer_id = ongoing_request.choose_next_peer(&self.connections);
        if let Some(peer_id) = peer_id {
            let outbound_request_id =
                behaviour.send_request(&peer_id, ongoing_request.request.clone());

            self.inner.insert(outbound_request_id, ongoing_request);

            Ok(peer_id)
        } else {
            Err(ongoing_request)
        }
    }
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    // requests
    request_id_counter: u64,
    pending_requests: VecDeque<(RequestId, Request)>,
    ongoing_requests: OngoingRequests,
    // responses
    db: Database,
    ongoing_response: Option<(
        request_response::ResponseChannel<Response>,
        JoinHandle<Response>,
    )>,
}

impl Behaviour {
    pub fn new(cfg: request_response::Config, db: Database) -> Self {
        Self {
            inner: InnerBehaviour::new([(STREAM_PROTOCOL, ProtocolSupport::Full)], cfg),
            //
            request_id_counter: 0,
            pending_requests: VecDeque::new(),
            ongoing_requests: OngoingRequests::default(),
            //
            db,
            ongoing_response: None,
        }
    }

    fn next_request_id(&mut self) -> RequestId {
        self.request_id_counter += 1;
        RequestId(self.request_id_counter)
    }

    pub fn request(&mut self, request: Request) -> RequestId {
        let request_id = self.next_request_id();
        self.pending_requests.push_back((request_id, request));
        request_id
    }

    fn read_db(&self, request: Request) -> JoinHandle<Response> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || match request {
            Request::BlockEndProgramStates(block_hash) => Response::BlockEndProgramStates {
                block_hash,
                states: db.block_end_program_states(block_hash).unwrap_or_default(),
            },
            Request::DataForHashes(hashes) => Response::DataForHashes(
                hashes
                    .into_iter()
                    .filter_map(|hash| Some((hash, db.read_by_hash(hash)?)))
                    .collect(),
            ),
            Request::ProgramCodeIds(ids) => Response::ProgramCodeIds(
                ids.into_iter()
                    .filter_map(|program_id| Some((program_id, db.program_code_id(program_id)?)))
                    .collect(),
            ),
        })
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<Request, Response>,
    ) -> Poll<ToSwarm<Event, THandlerInEvent<Self>>> {
        match event {
            request_response::Event::Message {
                peer: _,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                self.ongoing_response = Some((channel, self.read_db(request)));
            }
            request_response::Event::Message {
                peer,
                message:
                    Message::Response {
                        request_id,
                        response,
                    },
            } => {
                let ongoing_request = self
                    .ongoing_requests
                    .remove(request_id)
                    .expect("unknown response");

                let request_id = ongoing_request.request_id;
                let event = match ongoing_request.try_complete(peer, response) {
                    Ok(OngoingRequestCompletion::Full(response)) => Event::RequestSucceed {
                        request_id,
                        response,
                    },
                    Ok(OngoingRequestCompletion::Partial(new_ongoing_request)) => {
                        match self
                            .ongoing_requests
                            .send_request(&mut self.inner, new_ongoing_request)
                        {
                            Ok(peer_id) => Event::NewRequestRound {
                                request_id,
                                peer_id,
                            },
                            Err(new_ongoing_request) => Event::RequestSucceed {
                                request_id,
                                response: new_ongoing_request
                                    .response
                                    .expect("`try_complete` called above"),
                            },
                        }
                    }
                    Err(error) => Event::RequestFailed { request_id, error },
                };

                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
            request_response::Event::OutboundFailure {
                peer,
                request_id: _,
                error: OutboundFailure::UnsupportedProtocols,
            } => {
                log::debug!("Request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol. Disconnecting...");
                return Poll::Ready(ToSwarm::CloseConnection {
                    peer_id: peer,
                    connection: CloseConnection::All,
                });
            }
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
            } => {
                log::trace!("outbound failure for request {request_id} to {peer}: {error}");

                let ongoing_request = self
                    .ongoing_requests
                    .remove(request_id)
                    .expect("unknown response");

                let request_id = ongoing_request.request_id;

                let new_ongoing_request = ongoing_request.peer_failed(peer);
                let event = match self
                    .ongoing_requests
                    .send_request(&mut self.inner, new_ongoing_request)
                {
                    Ok(peer_id) => Event::NewRequestRound {
                        request_id,
                        peer_id,
                    },
                    Err(new_ongoing_request) => match new_ongoing_request.response {
                        Some(response) => Event::RequestSucceed {
                            request_id,
                            response,
                        },
                        None => Event::RequestFailed {
                            request_id,
                            error: RequestFailure::NoAvailablePeers,
                        },
                    },
                };
                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
            request_response::Event::InboundFailure {
                peer,
                request_id: _,
                error: InboundFailure::UnsupportedProtocols,
            } => {
                log::debug!("Request from {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol. Disconnecting...");
                return Poll::Ready(ToSwarm::CloseConnection {
                    peer_id: peer,
                    connection: CloseConnection::All,
                });
            }
            request_response::Event::InboundFailure { .. } => {}
            request_response::Event::ResponseSent { .. } => {}
        }

        Poll::Pending
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<InnerBehaviour>;
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
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_outbound_connection(connection_id, peer, addr, role_override)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event);
        self.ongoing_requests.on_swarm_event(event);
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
        if let Some((request_id, request)) = self.pending_requests.pop_back() {
            let ongoing_request = OngoingRequest::new(request_id, request);

            match self
                .ongoing_requests
                .send_request(&mut self.inner, ongoing_request)
            {
                Ok(peer_id) => {
                    return Poll::Ready(ToSwarm::GenerateEvent(Event::NewRequestRound {
                        request_id,
                        peer_id,
                    }));
                }
                Err(ongoing_request) => {
                    self.pending_requests
                        .push_back((request_id, ongoing_request.request));
                }
            }
        }

        if let Some((channel, mut db_reader)) = self.ongoing_response.take() {
            if let Poll::Ready(data) = db_reader.poll_unpin(cx) {
                let resp = data.expect("database panicked");
                let _res = self.inner.send_response(channel, resp);
            } else {
                self.ongoing_response = Some((channel, db_reader));
            }
        }

        if let Poll::Ready(to_swarm) = self.inner.poll(cx) {
            return match to_swarm {
                ToSwarm::GenerateEvent(event) => self.handle_inner_event(event),
                to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                    unreachable!("`ToSwarm::GenerateEvent` is handled above")
                })),
            };
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use ethexe_db::MemDb;
    use libp2p::Swarm;
    use libp2p_swarm_test::SwarmExt;

    async fn new_swarm() -> (Swarm<Behaviour>, Database) {
        let db = Database::from_one(&MemDb::default());
        let behaviour = Behaviour::new(request_response::Config::default(), db.clone());
        let mut swarm = Swarm::new_ephemeral(move |_keypair| behaviour);
        swarm.listen().with_memory_addr_external().await;
        (swarm, db)
    }

    #[test]
    fn validate_excessive_data() {
        let hash1 = ethexe_db::hash(b"1");
        let hash2 = ethexe_db::hash(b"2");
        let hash3 = ethexe_db::hash(b"3");

        let request = Request::DataForHashes([hash1, hash2].into());
        let response = Response::DataForHashes(
            [
                (hash1, b"1".to_vec()),
                (hash2, b"2".to_vec()),
                (hash3, b"3".to_vec()),
            ]
            .into(),
        );
        assert_eq!(
            request.validate_response(&response),
            Err(RequestFailure::ExcessiveData)
        );

        let request = Request::ProgramCodeIds([ProgramId::from(1), ProgramId::from(2)].into());
        let response = Response::ProgramCodeIds(
            [
                (ProgramId::from(1), CodeId::default()),
                (ProgramId::from(2), CodeId::default()),
                (ProgramId::from(3), CodeId::default()),
            ]
            .into(),
        );
        assert_eq!(
            request.validate_response(&response),
            Err(RequestFailure::ExcessiveData)
        );
    }

    #[test]
    fn validate_data_hash_mismatch() {
        let hash1 = ethexe_db::hash(b"1");

        let request = Request::DataForHashes([hash1].into());
        let response = Response::DataForHashes([(hash1, b"2".to_vec())].into());
        assert_eq!(
            request.validate_response(&response),
            Err(RequestFailure::DataHashMismatch)
        );
    }

    #[tokio::test]
    async fn smoke() {
        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, bob_db) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();

        let hello_hash = bob_db.write(b"hello");
        let world_hash = bob_db.write(b"world");

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let request_id = alice
            .behaviour_mut()
            .request(Request::DataForHashes([hello_hash, world_hash].into()));

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::NewRequestRound {
                request_id,
                peer_id: bob_peer_id,
            }
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                request_id,
                response: Response::DataForHashes(
                    [
                        (hello_hash, b"hello".to_vec()),
                        (world_hash, b"world".to_vec())
                    ]
                    .into()
                )
            }
        )
    }

    #[tokio::test]
    async fn request_response_type_mismatch() {
        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let mut bob = Swarm::new_ephemeral(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let request_id = alice
            .behaviour_mut()
            .request(Request::DataForHashes([].into()));

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::NewRequestRound {
                request_id,
                peer_id: *bob.local_peer_id(),
            }
        );

        loop {
            tokio::select! {
                event = bob.next_behaviour_event() => {
                    match event {
                        request_response::Event::Message {
                            message:
                                Message::Request {
                                    channel, request, ..
                                },
                            ..
                        } => {
                            assert_eq!(request, Request::DataForHashes([].into()));
                            let _res = bob
                                .behaviour_mut()
                                .send_response(channel, Response::ProgramCodeIds([].into()));
                        }
                        request_response::Event::ResponseSent { .. } => continue,
                        e => unreachable!("unexpected event: {:?}", e),
                    }
                }
                event = alice.next_behaviour_event() => {
                    assert_eq!(
                        event,
                        Event::RequestFailed {
                            request_id,
                            error: RequestFailure::TypeMismatch
                        }
                    );
                    break;
                }
            }
        }
    }

    #[tokio::test]
    async fn request_completed_by_2_rounds() {
        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, bob_db) = new_swarm().await;
        let (mut charlie, charlie_db) = new_swarm().await;

        alice.connect(&mut bob).await;
        alice.connect(&mut charlie).await;
        tokio::spawn(bob.loop_on_next());
        tokio::spawn(charlie.loop_on_next());

        let hello_hash = bob_db.write(b"hello");
        let world_hash = charlie_db.write(b"world");

        let request_id = alice
            .behaviour_mut()
            .request(Request::DataForHashes([hello_hash, world_hash].into()));

        // first round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, .. } if rid == request_id)
        );
        // second round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, .. } if rid == request_id)
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                request_id,
                response: Response::DataForHashes(
                    [
                        (hello_hash, b"hello".to_vec()),
                        (world_hash, b"world".to_vec())
                    ]
                    .into()
                )
            }
        );
    }
}
