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

mod ongoing;

use crate::{
    db_sync::ongoing::{
        OngoingRequests, OngoingResponses, PeerFailed, PeerResponse, SendRequestError,
        SendRequestErrorKind,
    },
    export::{Multiaddr, PeerId},
    utils::ParityScaleCodec,
};
use ethexe_db::Database;
use gear_core::ids::ProgramId;
use gprimitives::H256;
use libp2p::{
    core::Endpoint,
    request_response,
    request_response::{InboundFailure, Message, OutboundFailure, ProtocolSupport},
    swarm::{
        CloseConnection, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    StreamProtocol,
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet},
    task::{Context, Poll},
    time::Duration,
};

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct RequestId(u64);

#[derive(Debug, Eq, PartialEq)]
enum RequestValidationError {
    /// Request kind unequal to response kind
    TypeMismatch,
    /// Response contains more data than requested
    ExcessiveData,
    /// Hashed data unequal to its corresponding hash
    DataHashMismatch,
}

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode)]
pub enum Request {
    DataForHashes(BTreeSet<H256>),
    ProgramIds,
}

impl Request {
    fn validate_response(&self, resp: &Response) -> Result<(), RequestValidationError> {
        match (self, resp) {
            (Request::DataForHashes(requested_hashes), Response::DataForHashes(hashes)) => {
                for (hash, data) in hashes {
                    if !requested_hashes.contains(hash) {
                        return Err(RequestValidationError::ExcessiveData);
                    }

                    if *hash != ethexe_db::hash(data) {
                        return Err(RequestValidationError::DataHashMismatch);
                    }
                }

                Ok(())
            }
            (Request::ProgramIds, Response::ProgramIds(_ids)) => Ok(()),
            (_, _) => Err(RequestValidationError::TypeMismatch),
        }
    }

    /// Calculate missing request keys in response and create a new request with these keys
    fn difference(&self, resp: &Response) -> Option<Self> {
        match (self, resp) {
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
            (Request::ProgramIds, Response::ProgramIds(_ids)) => None,
            _ => unreachable!("should be checked in `validate_response`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Encode, Decode)]
pub enum Response {
    /// Key (hash) - value (bytes) data
    DataForHashes(BTreeMap<H256, Vec<u8>>),
    /// All existing programs
    ProgramIds(BTreeSet<ProgramId>),
}

impl Response {
    fn merge(self, new_response: Response) -> Response {
        match (self, new_response) {
            (Response::DataForHashes(mut data), Response::DataForHashes(new_data)) => {
                data.extend(new_data);
                Response::DataForHashes(data)
            }
            (Response::ProgramIds(mut ids), Response::ProgramIds(new_ids)) => {
                ids.extend(new_ids);
                Response::ProgramIds(ids)
            }
            _ => unreachable!("should be checked in `validate_response`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum NewRequestRoundReason {
    /// Request was queued for the first time or re-queued
    FromQueue,
    /// We have only part of the data
    PartialData,
    /// Peer failed to respond
    PeerFailed,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RequestFailure {
    OutOfRounds,
    Timeout,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    NewRequestRound {
        /// The ID of request
        request_id: RequestId,
        /// Peer we're currently requesting to
        peer_id: PeerId,
        /// Reason for new request round
        reason: NewRequestRoundReason,
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

#[derive(Debug, Clone)]
pub struct Config {
    max_rounds_per_request: u32,
    request_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_rounds_per_request: 10,
            request_timeout: Duration::from_secs(100),
        }
    }
}

impl Config {
    pub fn with_max_rounds_per_request(mut self, max_rounds_per_request: u32) -> Self {
        self.max_rounds_per_request = max_rounds_per_request;
        self
    }

    pub fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    ongoing_requests: OngoingRequests,
    ongoing_responses: OngoingResponses,
}

impl Behaviour {
    pub fn new(config: Config, db: Database) -> Self {
        Self {
            inner: InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
            ongoing_requests: OngoingRequests::from_config(&config),
            ongoing_responses: OngoingResponses::from_db(db),
        }
    }

    pub fn request(&mut self, request: Request) -> RequestId {
        self.ongoing_requests.push_pending_request(request)
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
                self.ongoing_responses.prepare_response(channel, request);
            }
            request_response::Event::Message {
                peer,
                message:
                    Message::Response {
                        request_id,
                        response,
                    },
            } => {
                let event = match self.ongoing_requests.on_peer_response(
                    &mut self.inner,
                    peer,
                    request_id,
                    response,
                ) {
                    Ok((request_id, response)) => Event::RequestSucceed {
                        request_id,
                        response,
                    },
                    Err(PeerResponse::NewRound {
                        peer_id,
                        request_id,
                    }) => Event::NewRequestRound {
                        request_id,
                        peer_id,
                        reason: NewRequestRoundReason::PartialData,
                    },
                    Err(PeerResponse::SendRequest(SendRequestError {
                        request_id,
                        kind: SendRequestErrorKind::OutOfRounds,
                    })) => Event::RequestFailed {
                        request_id,
                        error: RequestFailure::OutOfRounds,
                    },
                    Err(PeerResponse::SendRequest(SendRequestError {
                        request_id: _,
                        kind: SendRequestErrorKind::Pending,
                    })) => return Poll::Pending,
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

                let event =
                    match self
                        .ongoing_requests
                        .on_peer_failed(&mut self.inner, peer, request_id)
                    {
                        Ok((peer_id, request_id)) => Event::NewRequestRound {
                            request_id,
                            peer_id,
                            reason: NewRequestRoundReason::PeerFailed,
                        },
                        Err(PeerFailed::SendRequest(SendRequestError {
                            request_id,
                            kind: SendRequestErrorKind::OutOfRounds,
                        })) => Event::RequestFailed {
                            request_id,
                            error: RequestFailure::OutOfRounds,
                        },
                        Err(PeerFailed::SendRequest(SendRequestError {
                            request_id: _,
                            kind: SendRequestErrorKind::Pending,
                        })) => return Poll::Pending,
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
        if let Some(request_id) = self.ongoing_requests.remove_if_timeout(cx) {
            return Poll::Ready(ToSwarm::GenerateEvent(Event::RequestFailed {
                request_id,
                error: RequestFailure::Timeout,
            }));
        }

        let event = match self.ongoing_requests.send_pending_request(&mut self.inner) {
            Ok(Some((peer_id, request_id))) => Some(Event::NewRequestRound {
                request_id,
                peer_id,
                reason: NewRequestRoundReason::FromQueue,
            }),
            Ok(None) => None,
            Err(SendRequestError {
                request_id,
                kind: SendRequestErrorKind::OutOfRounds,
            }) => Some(Event::RequestFailed {
                request_id,
                error: RequestFailure::OutOfRounds,
            }),
            Err(SendRequestError {
                request_id: _,
                kind: SendRequestErrorKind::Pending,
            }) => None,
        };
        if let Some(event) = event {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        self.ongoing_responses
            .poll_send_response(cx, &mut self.inner);

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
    use ethexe_db::{CodesStorage, MemDb};
    use gprimitives::CodeId;
    use libp2p::{futures::StreamExt, Swarm};
    use libp2p_swarm_test::SwarmExt;
    use std::mem;

    async fn new_swarm_with_config(config: Config) -> (Swarm<Behaviour>, Database) {
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let behaviour = Behaviour::new(config, db.clone());
        let mut swarm = Swarm::new_ephemeral(move |_keypair| behaviour);
        swarm.listen().with_memory_addr_external().await;
        (swarm, db)
    }

    async fn new_swarm() -> (Swarm<Behaviour>, Database) {
        new_swarm_with_config(Config::default()).await
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
            Err(RequestValidationError::ExcessiveData)
        );
    }

    #[test]
    fn validate_data_hash_mismatch() {
        let hash1 = ethexe_db::hash(b"1");

        let request = Request::DataForHashes([hash1].into());
        let response = Response::DataForHashes([(hash1, b"2".to_vec())].into());
        assert_eq!(
            request.validate_response(&response),
            Err(RequestValidationError::DataHashMismatch)
        );
    }

    #[tokio::test]
    async fn smoke() {
        const PID1: ProgramId = ProgramId::new([1; 32]);
        const PID2: ProgramId = ProgramId::new([2; 32]);

        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, bob_db) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();

        bob_db.set_program_code_id(PID1, CodeId::zero());
        bob_db.set_program_code_id(PID2, CodeId::zero());

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let request_id = alice.behaviour_mut().request(Request::ProgramIds);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::NewRequestRound {
                request_id,
                peer_id: bob_peer_id,
                reason: NewRequestRoundReason::FromQueue,
            }
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                request_id,
                response: Response::ProgramIds([PID1, PID2].into())
            }
        )
    }

    #[tokio::test]
    async fn out_of_rounds() {
        init_logger();

        let alice_config = Config::default().with_max_rounds_per_request(1);
        let (mut alice, _alice_db) = new_swarm_with_config(alice_config).await;

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
                reason: NewRequestRoundReason::FromQueue,
            }
        );

        tokio::spawn(async move {
            while let Some(event) = bob.next().await {
                if let Ok(request_response::Event::Message {
                    message:
                        Message::Request {
                            channel, request, ..
                        },
                    ..
                }) = event.try_into_behaviour_event()
                {
                    assert_eq!(request, Request::DataForHashes([].into()));
                    let _res = bob
                        .behaviour_mut()
                        .send_response(channel, Response::ProgramIds([].into()));
                }
            }
        });

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestFailed {
                request_id,
                error: RequestFailure::OutOfRounds,
            }
        );
    }

    #[tokio::test]
    async fn timeout() {
        init_logger();

        let alice_config = Config::default().with_request_timeout(Duration::from_secs(2));
        let (mut alice, _alice_db) = new_swarm_with_config(alice_config).await;

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
                reason: NewRequestRoundReason::FromQueue,
            }
        );

        tokio::spawn(async move {
            while let Some(event) = bob.next().await {
                if let Ok(request_response::Event::Message {
                    message:
                        Message::Request {
                            channel, request, ..
                        },
                    ..
                }) = event.try_into_behaviour_event()
                {
                    assert_eq!(request, Request::DataForHashes([].into()));
                    // just ignore request
                    mem::forget(channel);
                }
            }
        });

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestFailed {
                request_id,
                error: RequestFailure::Timeout,
            }
        );
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
            matches!(event, Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::FromQueue, .. } if rid == request_id)
        );
        // second round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::PartialData, .. } if rid == request_id)
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
