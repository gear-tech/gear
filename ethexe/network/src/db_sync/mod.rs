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

pub use ongoing::ValidatingResponse;

use crate::{
    db_sync::ongoing::{
        ExternalValidation, OngoingRequests, OngoingResponses, PeerResponse, SendRequestError,
        SendRequestErrorKind,
    },
    export::{Multiaddr, PeerId},
    peer_score,
    utils::ParityScaleCodec,
};
use ethexe_db::Database;
use gear_core::ids::ProgramId;
use gprimitives::H256;
use libp2p::{
    core::{transport::PortUse, Endpoint},
    request_response,
    request_response::{InboundFailure, Message, OutboundFailure, ProtocolSupport},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
    StreamProtocol,
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    task::{Context, Poll},
    time::Duration,
};

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct RequestId(u64);

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct ResponseId(u64);

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode)]
pub enum Request {
    DataForHashes(BTreeSet<H256>),
    ProgramIds,
}

impl Request {
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
            _ => unreachable!("should be checked in `Response::validate()`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ResponseValidationError {
    /// Request kind unequal to response kind
    TypeMismatch,
    /// Hashed data unequal to its corresponding hash
    DataHashMismatch,
}

#[derive(Debug, Eq, PartialEq, Encode, Decode)]
pub enum Response {
    /// Key (hash) - value (bytes) data
    DataForHashes(BTreeMap<H256, Vec<u8>>),
    /// All existing programs
    ProgramIds(BTreeSet<ProgramId>),
}

impl Response {
    fn merge(&mut self, new_response: Response) {
        match (self, new_response) {
            (Response::DataForHashes(data), Response::DataForHashes(new_data)) => {
                data.extend(new_data);
            }
            (Response::ProgramIds(ids), Response::ProgramIds(new_ids)) => {
                ids.extend(new_ids);
            }
            _ => unreachable!("should be checked in `Response::validate()`"),
        }
    }

    /// Validates response against request.
    ///
    /// Returns `false` if external validation is required.
    fn validate(&self, request: &Request) -> Result<bool, ResponseValidationError> {
        match (request, self) {
            (Request::DataForHashes(_requested_hashes), Response::DataForHashes(hashes)) => {
                for (hash, data) in hashes {
                    if *hash != ethexe_db::hash(data) {
                        return Err(ResponseValidationError::DataHashMismatch);
                    }
                }

                Ok(true)
            }
            (Request::ProgramIds, Response::ProgramIds(_ids)) => Ok(false),
            (_, _) => Err(ResponseValidationError::TypeMismatch),
        }
    }

    fn strip(&mut self, request: &Request) -> bool {
        match (request, self) {
            (Request::DataForHashes(requested_hashes), Self::DataForHashes(hashes)) => {
                let hashes_keys: BTreeSet<H256> = hashes.keys().copied().collect();
                let excessive_requested_hashes: BTreeSet<H256> =
                    hashes_keys.difference(requested_hashes).copied().collect();

                if excessive_requested_hashes.is_empty() {
                    return false;
                }

                for excessive_key in excessive_requested_hashes {
                    hashes.remove(&excessive_key);
                }

                true
            }
            (Request::ProgramIds, Response::ProgramIds(_ids)) => false,
            _ => unreachable!("should be checked in `Response::validate()`"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum NewRequestRoundReason {
    /// Request was queued for the first time or re-queued because of there are no available peers
    FromQueue,
    /// We have only part of the data
    PartialData,
    /// Peer failed to respond or response validation failed
    PeerFailed,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RequestFailure {
    /// Request exceeded its round limit
    OutOfRounds,
    /// Request had been processing for too long
    Timeout,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    /// Request is processing new round
    NewRequestRound {
        /// The ID of request
        request_id: RequestId,
        /// Peer we're currently requesting to
        peer_id: PeerId,
        /// Reason for new request round
        reason: NewRequestRoundReason,
    },
    /// Request is in pending state because of lack of available peers
    PendingStateRequest {
        //// The ID of request
        request_id: RequestId,
    },
    /// External validation is mandatory for response
    ExternalValidation(ValidatingResponse),
    /// Request completion done
    RequestSucceed {
        /// The ID of request
        request_id: RequestId,
        /// Response to the request itself
        response: Response,
    },
    /// Request failed
    RequestFailed {
        /// The ID of request
        request_id: RequestId,
        /// Reason of request failure
        error: RequestFailure,
    },
    /// Incoming request
    IncomingRequest {
        /// The ID of in-progress response
        response_id: ResponseId,
        /// Peer who requested
        peer_id: PeerId,
    },
    /// Request dropped because simultaneous limit exceeded
    IncomingRequestDropped {
        /// Peer who should have received the response
        peer_id: PeerId,
    },
    /// Response sent to incoming request
    ResponseSent {
        /// The ID of completed response
        response_id: ResponseId,
        /// Peer who should receive response
        peer_id: PeerId,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    max_rounds_per_request: u32,
    request_timeout: Duration,
    max_simultaneous_responses: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_rounds_per_request: 10,
            request_timeout: Duration::from_secs(100),
            max_simultaneous_responses: 10,
        }
    }
}

#[cfg(test)] // used only in tests yet
impl Config {
    pub(crate) fn with_max_rounds_per_request(mut self, max_rounds_per_request: u32) -> Self {
        self.max_rounds_per_request = max_rounds_per_request;
        self
    }

    pub(crate) fn with_request_timeout(mut self, request_timeout: Duration) -> Self {
        self.request_timeout = request_timeout;
        self
    }

    pub(crate) fn with_max_simultaneous_responses(
        mut self,
        max_simultaneous_responses: u32,
    ) -> Self {
        self.max_simultaneous_responses = max_simultaneous_responses;
        self
    }
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    pending_events: VecDeque<Event>,
    peer_score_handle: peer_score::Handle,
    ongoing_requests: OngoingRequests,
    ongoing_responses: OngoingResponses,
}

impl Behaviour {
    /// TODO: use database via traits
    pub(crate) fn new(config: Config, peer_score_handle: peer_score::Handle, db: Database) -> Self {
        Self {
            inner: InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
            pending_events: VecDeque::new(),
            peer_score_handle: peer_score_handle.clone(),
            ongoing_requests: OngoingRequests::new(&config, peer_score_handle),
            ongoing_responses: OngoingResponses::new(db, &config),
        }
    }

    pub(crate) fn request(&mut self, request: Request) -> RequestId {
        self.ongoing_requests.push_pending_request(request)
    }

    pub(crate) fn request_validated(
        &mut self,
        res: Result<ValidatingResponse, ValidatingResponse>,
    ) {
        let res = self
            .ongoing_requests
            .on_external_validation(res, &mut self.inner);
        let event = match res {
            Ok(ExternalValidation::Success {
                request_id,
                response,
            }) => Event::RequestSucceed {
                request_id,
                response,
            },
            Ok(ExternalValidation::NewRound {
                peer_id,
                request_id,
            }) => Event::NewRequestRound {
                request_id,
                peer_id,
                reason: NewRequestRoundReason::PartialData,
            },
            Err(SendRequestError {
                request_id,
                kind: SendRequestErrorKind::OutOfRounds,
            }) => Event::RequestFailed {
                request_id,
                error: RequestFailure::OutOfRounds,
            },
            Err(SendRequestError {
                request_id,
                kind: SendRequestErrorKind::NoPeers,
            }) => Event::PendingStateRequest { request_id },
        };
        self.pending_events.push_back(event);
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<Request, Response>,
    ) -> Poll<ToSwarm<Event, THandlerInEvent<Self>>> {
        match event {
            request_response::Event::Message {
                peer,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                let response_id = self
                    .ongoing_responses
                    .prepare_response(peer, channel, request);

                let event = if let Some(response_id) = response_id {
                    Event::IncomingRequest {
                        response_id,
                        peer_id: peer,
                    }
                } else {
                    Event::IncomingRequestDropped { peer_id: peer }
                };

                return Poll::Ready(ToSwarm::GenerateEvent(event));
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
                    Ok(PeerResponse::Success {
                        request_id,
                        response,
                    }) => Event::RequestSucceed {
                        request_id,
                        response,
                    },
                    Ok(PeerResponse::NewRound {
                        peer_id,
                        request_id,
                    }) => Event::NewRequestRound {
                        request_id,
                        peer_id,
                        reason: NewRequestRoundReason::PartialData,
                    },
                    Ok(PeerResponse::ExternalValidation(validating_response)) => {
                        Event::ExternalValidation(validating_response)
                    }
                    Err(SendRequestError {
                        request_id,
                        kind: SendRequestErrorKind::OutOfRounds,
                    }) => Event::RequestFailed {
                        request_id,
                        error: RequestFailure::OutOfRounds,
                    },
                    Err(SendRequestError {
                        request_id,
                        kind: SendRequestErrorKind::NoPeers,
                    }) => Event::PendingStateRequest { request_id },
                };

                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
            request_response::Event::OutboundFailure {
                peer,
                request_id,
                error,
            } => {
                log::trace!("outbound failure for request {request_id} to {peer}: {error}");

                if let OutboundFailure::UnsupportedProtocols = error {
                    log::debug!("request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol");
                    self.peer_score_handle.unsupported_protocol(peer);
                }

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
                        Err(SendRequestError {
                            request_id,
                            kind: SendRequestErrorKind::OutOfRounds,
                        }) => Event::RequestFailed {
                            request_id,
                            error: RequestFailure::OutOfRounds,
                        },
                        Err(SendRequestError {
                            request_id,
                            kind: SendRequestErrorKind::NoPeers,
                        }) => Event::PendingStateRequest { request_id },
                    };

                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
            request_response::Event::InboundFailure {
                peer,
                request_id: _,
                error: InboundFailure::UnsupportedProtocols,
            } => {
                log::debug!("request from {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol");
                self.peer_score_handle.unsupported_protocol(peer);
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

        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
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
                kind: SendRequestErrorKind::NoPeers,
            }) => None,
        };
        if let Some(event) = event {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        if let Poll::Ready((peer_id, response_id)) = self
            .ongoing_responses
            .poll_send_response(cx, &mut self.inner)
        {
            return Poll::Ready(ToSwarm::GenerateEvent(Event::ResponseSent {
                response_id,
                peer_id,
            }));
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
    use ethexe_db::{CodesStorage, MemDb};
    use gprimitives::CodeId;
    use libp2p::{futures::StreamExt, swarm::SwarmEvent, Swarm};
    use libp2p_swarm_test::SwarmExt;
    use std::{iter, mem};

    async fn new_swarm_with_config(config: Config) -> (Swarm<Behaviour>, Database) {
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let behaviour = Behaviour::new(config, peer_score::Handle::new_test(), db.clone());
        let mut swarm = Swarm::new_ephemeral(move |_keypair| behaviour);
        swarm.listen().with_memory_addr_external().await;
        (swarm, db)
    }

    async fn new_swarm() -> (Swarm<Behaviour>, Database) {
        new_swarm_with_config(Config::default()).await
    }

    #[test]
    fn validate_data_stripped() {
        let hash1 = ethexe_db::hash(b"1");
        let hash2 = ethexe_db::hash(b"2");
        let hash3 = ethexe_db::hash(b"3");

        let request = Request::DataForHashes([hash1, hash2].into());
        let mut response = Response::DataForHashes(
            [
                (hash1, b"1".to_vec()),
                (hash2, b"2".to_vec()),
                (hash3, b"3".to_vec()),
            ]
            .into(),
        );
        assert!(response.strip(&request));
        assert_eq!(
            response,
            Response::DataForHashes([(hash1, b"1".to_vec()), (hash2, b"2".to_vec())].into())
        );
    }

    #[test]
    fn validate_data_hash_mismatch() {
        let hash1 = ethexe_db::hash(b"1");

        let request = Request::DataForHashes([hash1].into());
        let response = Response::DataForHashes([(hash1, b"2".to_vec())].into());
        assert_eq!(
            response.validate(&request),
            Err(ResponseValidationError::DataHashMismatch)
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
        tokio::spawn(async move {
            let mut values = None;

            while let Some(event) = bob.next().await {
                let Ok(event) = event.try_into_behaviour_event() else {
                    continue;
                };

                match event {
                    Event::IncomingRequest {
                        response_id,
                        peer_id,
                    } => {
                        values = Some((response_id, peer_id));
                    }
                    Event::ResponseSent {
                        response_id,
                        peer_id,
                    } => {
                        let (initial_response_id, initial_peer_id) =
                            values.expect("IncomingRequest must be first");
                        assert_eq!(initial_response_id, response_id);
                        assert_eq!(initial_peer_id, peer_id);
                    }
                    _ => {}
                }
            }
        });

        let request_id = alice
            .behaviour_mut()
            .request(Request::DataForHashes([hello_hash, world_hash].into()));

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
    async fn excessive_data_stripped() {
        const DATA: [[u8; 1]; 3] = [*b"1", *b"2", *b"3"];

        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;

        let mut bob = Swarm::new_ephemeral(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let data_0 = ethexe_db::hash(&DATA[0]);
        let data_1 = ethexe_db::hash(&DATA[1]);
        let data_2 = ethexe_db::hash(&DATA[2]);

        let request_id = alice
            .behaviour_mut()
            .request(Request::DataForHashes([data_0, data_1].into()));

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
                    assert_eq!(request, Request::DataForHashes([data_0, data_1].into()));
                    bob.behaviour_mut()
                        .send_response(
                            channel,
                            Response::DataForHashes(
                                [
                                    (data_0, DATA[0].to_vec()),
                                    (data_1, DATA[1].to_vec()),
                                    (data_2, DATA[2].to_vec()),
                                ]
                                .into(),
                            ),
                        )
                        .unwrap();
                }
            }
        });

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                request_id,
                response: Response::DataForHashes(
                    [(data_0, DATA[0].to_vec()), (data_1, DATA[1].to_vec())].into()
                ),
            }
        );
    }

    #[tokio::test]
    async fn request_completed_by_3_rounds() {
        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, bob_db) = new_swarm().await;
        let (mut charlie, charlie_db) = new_swarm().await;
        let (mut dave, dave_db) = new_swarm().await;

        alice.connect(&mut bob).await;
        alice.connect(&mut charlie).await;
        alice.connect(&mut dave).await;
        tokio::spawn(bob.loop_on_next());
        tokio::spawn(charlie.loop_on_next());
        tokio::spawn(dave.loop_on_next());

        let hello_hash = bob_db.write(b"hello");
        let world_hash = charlie_db.write(b"world");
        let mark_hash = dave_db.write(b"!");

        let request_id = alice.behaviour_mut().request(Request::DataForHashes(
            [hello_hash, world_hash, mark_hash].into(),
        ));

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
        // third round
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
                        (world_hash, b"world".to_vec()),
                        (mark_hash, b"!".to_vec()),
                    ]
                    .into()
                )
            }
        );
    }

    #[tokio::test]
    async fn request_completed_after_new_peer() {
        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, bob_db) = new_swarm().await;
        let (charlie, charlie_db) = new_swarm().await;
        let charlie_addr = charlie.external_addresses().next().cloned().unwrap();

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

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

        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::PendingStateRequest { request_id: rid } if rid == request_id)
        );

        tokio::spawn(charlie.loop_on_next());
        alice.dial_and_wait(charlie_addr).await;

        // second round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::FromQueue, .. } if rid == request_id)
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
    async fn unsupported_protocol_handled() {
        init_logger();

        let alice_config = Config::default().with_request_timeout(Duration::from_secs(2));
        let (mut alice, _alice_db) = new_swarm_with_config(alice_config).await;

        let mut bob = Swarm::new_ephemeral(move |_keypair| {
            InnerBehaviour::new([], request_response::Config::default())
        });
        let bob_peer_id = *bob.local_peer_id();
        bob.connect(&mut alice).await;
        tokio::spawn(bob.loop_on_next());

        let request_id = alice.behaviour_mut().request(Request::ProgramIds);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::NewRequestRound {
                request_id,
                peer_id: bob_peer_id,
                reason: NewRequestRoundReason::FromQueue
            }
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::PendingStateRequest { request_id });

        let event = alice.next_swarm_event().await;
        assert!(
            matches!(event, SwarmEvent::ConnectionClosed { peer_id, .. } if peer_id == bob_peer_id)
        );
    }

    #[tokio::test]
    async fn simultaneous_responses_limit() {
        init_logger();

        let alice_config = Config::default().with_max_simultaneous_responses(2);
        let (mut alice, _alice_db) = new_swarm_with_config(alice_config).await;
        let (mut bob, _bob_db) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();
        alice.connect(&mut bob).await;

        // make request way heavier so there definitely will be a few simultaneous requests
        let request = Request::DataForHashes(
            iter::from_fn(|| Some(H256::random()))
                .take(16 * 1024)
                .collect(),
        );
        bob.behaviour_mut().request(request.clone());
        bob.behaviour_mut().request(request.clone());
        bob.behaviour_mut().request(request);
        tokio::spawn(bob.loop_on_next());

        let event = alice.next_behaviour_event().await;
        assert!(matches!(event, Event::IncomingRequest { peer_id, .. } if peer_id == bob_peer_id));

        let event = alice.next_behaviour_event().await;
        assert!(matches!(event, Event::IncomingRequest { peer_id, .. } if peer_id == bob_peer_id));

        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::IncomingRequestDropped { peer_id, .. } if peer_id == bob_peer_id),
            "event: {event:?}"
        );

        let event = alice.next_behaviour_event().await;
        assert!(matches!(event, Event::ResponseSent { peer_id, .. } if peer_id == bob_peer_id));

        let event = alice.next_behaviour_event().await;
        assert!(matches!(event, Event::ResponseSent { peer_id, .. } if peer_id == bob_peer_id));
    }

    #[tokio::test]
    async fn external_validation() {
        const PID1: ProgramId = ProgramId::new([1; 32]);
        const PID2: ProgramId = ProgramId::new([2; 32]);

        init_logger();

        let (mut alice, _alice_db) = new_swarm().await;
        let (mut bob, _bob_db) = new_swarm().await;
        let (mut charlie, charlie_db) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();
        let charlie_peer_id = *charlie.local_peer_id();

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        charlie_db.set_program_code_id(PID1, CodeId::zero());
        charlie_db.set_program_code_id(PID2, CodeId::zero());

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
        if let Event::ExternalValidation(validating_response) = event {
            assert_eq!(validating_response.peer_id(), bob_peer_id);
            let response = validating_response.response();
            assert_eq!(*response, Response::ProgramIds([].into()));
            alice
                .behaviour_mut()
                .request_validated(Err(validating_response));
        } else {
            unreachable!();
        }

        alice.connect(&mut charlie).await;
        tokio::spawn(charlie.loop_on_next());

        // `Event::NewRequestRound` skipped by `connect()` above

        let event = alice.next_behaviour_event().await;
        if let Event::ExternalValidation(validating_response) = event {
            assert_eq!(validating_response.peer_id(), charlie_peer_id);
            let response = validating_response.response();
            assert_eq!(*response, Response::ProgramIds([PID1, PID2].into()));
            alice
                .behaviour_mut()
                .request_validated(Ok(validating_response));
        } else {
            unreachable!();
        }

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestSucceed {
                request_id,
                response: Response::ProgramIds([PID1, PID2].into()),
            }
        );
    }
}
