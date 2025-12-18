// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

mod requests;
mod responses;

use crate::{db_sync::requests::OngoingRequests, utils::AlternateCollectionFmt};
pub(crate) use crate::{
    db_sync::{requests::RetriableRequest, responses::OngoingResponses},
    export::{Multiaddr, PeerId},
    peer_score,
    utils::ParityScaleCodec,
};
use async_trait::async_trait;
use ethexe_common::{
    db::{
        AnnounceStorageRO, BlockMetaStorageRO, CodesStorageRO, HashStorageRO, LatestDataStorageRO,
    },
    gear::CodeState,
    network::{AnnouncesRequest, AnnouncesResponse, CheckedAnnouncesResponse},
};
use ethexe_db::Database;
use futures::FutureExt;
use gprimitives::{ActorId, CodeId, H256};
use libp2p::{
    StreamProtocol,
    core::{Endpoint, transport::PortUse},
    request_response,
    request_response::{InboundFailure, Message, OutboundFailure, ProtocolSupport},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum NewRequestRoundReason {
    /// Request was queued for the first time or re-queued because of there are no available peers
    FromQueue,
    /// We have only part of the data
    PartialData,
    /// Peer failed to respond or response validation failed
    PeerFailed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, derive_more::Display)]
pub enum RequestFailure {
    /// Request exceeded its round limit
    #[display("Request exceeded its round limit")]
    OutOfRounds,
    /// Request had been processing for too long
    #[display("Request had been processing for too long")]
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
    /// Request completion done
    RequestSucceed {
        /// The ID of request
        request_id: RequestId,
    },
    /// Request failed
    RequestFailed {
        /// The failed request
        request_id: RequestId,
        /// Reason of request failure
        error: RequestFailure,
    },
    /// Request canceled
    ///
    /// User dropped [`HandleFuture`].
    ///
    /// NOTE: `Event` is not guaranteed in a multithreaded environment
    RequestCancelled {
        /// The canceled request
        request_id: RequestId,
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
    pub max_rounds_per_request: u32,
    pub request_timeout: Duration,
    pub max_simultaneous_responses: u32,
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

#[async_trait]
pub trait ExternalDataProvider: Send + Sync {
    fn clone_boxed(&self) -> Box<dyn ExternalDataProvider>;

    async fn programs_code_ids_at(
        self: Box<Self>,
        program_ids: BTreeSet<ActorId>,
        block: H256,
    ) -> anyhow::Result<Vec<CodeId>>;

    async fn codes_states_at(
        self: Box<Self>,
        code_ids: BTreeSet<CodeId>,
        block: H256,
    ) -> anyhow::Result<Vec<CodeState>>;
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct RequestId(u64);

impl RequestId {
    fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        RequestId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct ResponseId(pub(crate) u64);

#[derive(derive_more::Debug, Default, Clone, Eq, PartialEq, Encode, Decode, derive_more::From)]
pub struct HashesRequest(
    #[debug("{:?}", AlternateCollectionFmt::set(_0, "hashes"))] pub BTreeSet<H256>,
);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProgramIdsRequest {
    pub at: H256,
    pub expected_count: u64,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidCodesRequest {
    pub at: H256,
    pub validated_count: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum Request {
    Hashes(HashesRequest),
    ProgramIds(ProgramIdsRequest),
    ValidCodes(ValidCodesRequest),
    Announces(AnnouncesRequest),
}

impl Request {
    pub fn hashes(request: impl Into<BTreeSet<H256>>) -> Self {
        Self::Hashes(HashesRequest(request.into()))
    }

    pub fn program_ids(at: H256, expected_count: u64) -> Self {
        Self::ProgramIds(ProgramIdsRequest { at, expected_count })
    }

    pub fn valid_codes(at: H256, validated_count: u64) -> Self {
        Self::ValidCodes(ValidCodesRequest {
            at,
            validated_count,
        })
    }
}

#[derive(derive_more::Debug, Clone, Eq, PartialEq, derive_more::From, derive_more::Unwrap)]
pub enum Response {
    Hashes(#[debug("{:?}", AlternateCollectionFmt::map(_0, "entries"))] BTreeMap<H256, Vec<u8>>),
    ProgramIds(
        #[debug("{:?}", AlternateCollectionFmt::map(_0, "programs"))] BTreeMap<ActorId, CodeId>,
    ),
    ValidCodes(#[debug("{:?}", AlternateCollectionFmt::set(_0, "codes"))] BTreeSet<CodeId>),
    Announces(CheckedAnnouncesResponse),
}

pub type HandleResult = Result<Response, (RequestFailure, RetriableRequest)>;

enum HandleAction {
    Request(RequestId, Request),
    Retry(RetriableRequest),
}

impl HandleAction {
    fn request_id(&self) -> RequestId {
        match self {
            HandleAction::Request(request_id, _) => *request_id,
            HandleAction::Retry(request) => request.id(),
        }
    }
}

pub struct HandleFuture {
    request_id: RequestId,
    rx: oneshot::Receiver<HandleResult>,
}

impl HandleFuture {
    pub fn request_id(&self) -> RequestId {
        self.request_id
    }
}

impl Future for HandleFuture {
    type Output = HandleResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.rx
            .poll_unpin(cx)
            .map(|res| res.expect("channel should never be closed"))
    }
}

#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<(HandleAction, oneshot::Sender<HandleResult>)>);

impl Handle {
    fn send(&self, action: HandleAction) -> HandleFuture {
        let (tx, rx) = oneshot::channel();
        let request_id = action.request_id();

        self.0
            .send((action, tx))
            .expect("channel should never be closed");

        HandleFuture { request_id, rx }
    }

    pub fn request(&self, request: Request) -> HandleFuture {
        self.send(HandleAction::Request(RequestId::next(), request))
    }

    pub fn retry(&self, request: RetriableRequest) -> HandleFuture {
        self.send(HandleAction::Retry(request))
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode)]
pub(crate) struct InnerProgramIdsRequest {
    at: H256,
}

/// Network-only type to be encoded-decoded and sent over the network
#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode, derive_more::From)]
pub(crate) enum InnerRequest {
    Hashes(HashesRequest),
    ProgramIds(InnerProgramIdsRequest),
    ValidCodes,
    Announces(AnnouncesRequest),
}

#[derive(Debug, Default, Eq, PartialEq, Encode, Decode)]
pub(crate) struct InnerHashesResponse(BTreeMap<H256, Vec<u8>>);

#[derive(Debug, Default, Eq, PartialEq, Encode, Decode)]
pub(crate) struct InnerProgramIdsResponse(BTreeSet<ActorId>);

/// Network-only type to be encoded-decoded and sent over the network
#[derive(Debug, Eq, PartialEq, derive_more::From, Encode, Decode)]
pub(crate) enum InnerResponse {
    Hashes(InnerHashesResponse),
    ProgramIds(InnerProgramIdsResponse),
    ValidCodes(BTreeSet<CodeId>),
    Announces(AnnouncesResponse),
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<InnerRequest, InnerResponse>>;

#[auto_impl::auto_impl(&, Box)]
pub trait DbSyncDatabase:
    Send + HashStorageRO + LatestDataStorageRO + BlockMetaStorageRO + AnnounceStorageRO + CodesStorageRO
{
    fn clone_boxed(&self) -> Box<dyn DbSyncDatabase>;
}

impl DbSyncDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn DbSyncDatabase> {
        Box::new(self.clone())
    }
}

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(HandleAction, oneshot::Sender<HandleResult>)>,
    peer_score_handle: peer_score::Handle,
    ongoing_requests: OngoingRequests,
    ongoing_responses: OngoingResponses,
}

impl Behaviour {
    pub(crate) fn new(
        config: Config,
        peer_score_handle: peer_score::Handle,
        external_data_provider: Box<dyn ExternalDataProvider>,
        db: Box<dyn DbSyncDatabase>,
    ) -> Self {
        let (handle, rx) = mpsc::unbounded_channel();
        let handle = Handle(handle);

        Self {
            inner: InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
            handle,
            rx,
            peer_score_handle: peer_score_handle.clone(),
            ongoing_requests: OngoingRequests::new(
                &config,
                peer_score_handle,
                external_data_provider,
            ),
            ongoing_responses: OngoingResponses::new(db, &config),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<InnerRequest, InnerResponse>,
    ) -> Poll<ToSwarm<Event, THandlerInEvent<Self>>> {
        match event {
            request_response::Event::Message {
                peer,
                connection_id: _,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                let response_id = self
                    .ongoing_responses
                    .handle_response(peer, channel, request);

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
                peer: _,
                connection_id: _,
                message:
                    Message::Response {
                        request_id,
                        response,
                    },
            } => {
                self.ongoing_requests.on_peer_response(request_id, response);
            }
            request_response::Event::OutboundFailure {
                peer,
                connection_id: _,
                request_id,
                error,
            } => {
                log::trace!("outbound failure for request {request_id} to {peer}: {error}");

                if let OutboundFailure::UnsupportedProtocols = error {
                    log::debug!(
                        "request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                    );
                    self.peer_score_handle.unsupported_protocol(peer);
                }

                self.ongoing_requests.on_peer_failure(request_id);
            }
            request_response::Event::InboundFailure {
                peer,
                connection_id: _,
                request_id: _,
                error: InboundFailure::UnsupportedProtocols,
            } => {
                log::debug!(
                    "request from {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                );
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
        if let Poll::Ready(Some((action, channel))) = self.rx.poll_recv(cx) {
            match action {
                HandleAction::Request(request_id, request) => {
                    self.ongoing_requests.request(request_id, request, channel);
                }
                HandleAction::Retry(request) => {
                    self.ongoing_requests.retry(request, channel);
                }
            }
        }

        if let Poll::Ready(request_event) = self.ongoing_requests.poll(cx, &mut self.inner) {
            return Poll::Ready(ToSwarm::GenerateEvent(request_event));
        }

        if let Poll::Ready((peer_id, response_id)) =
            self.ongoing_responses.poll(cx, &mut self.inner)
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

pub mod test_utils {
    use super::*;

    pub struct HandleStub {
        handle: Handle,
        rx: mpsc::UnboundedReceiver<(HandleAction, oneshot::Sender<HandleResult>)>,
    }

    impl Default for HandleStub {
        fn default() -> Self {
            Self::new()
        }
    }

    impl HandleStub {
        pub fn new() -> Self {
            let (tx, rx) = mpsc::unbounded_channel();
            Self {
                handle: Handle(tx),
                rx,
            }
        }

        pub fn handle(&self) -> Handle {
            self.handle.clone()
        }

        pub async fn recv_request(
            &mut self,
        ) -> (RequestId, Request, oneshot::Sender<HandleResult>) {
            match self.rx.recv().await.expect("handle channel closed") {
                (HandleAction::Request(id, request), tx) => (id, request, tx),
                (HandleAction::Retry(_), _) => panic!("unexpected retry in HandleStub"),
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{tests::DataProvider, utils::tests::init_logger};
    use assert_matches::assert_matches;
    use ethexe_common::{Announce, HashOf, StateHashWithQueueSize, db::*};
    use ethexe_db::{Database, MemDb};
    use libp2p::{
        Swarm, Transport,
        core::{transport::MemoryTransport, upgrade::Version},
        futures::StreamExt,
        identity::Keypair,
        swarm,
        swarm::SwarmEvent,
    };
    use libp2p_swarm_test::SwarmExt;
    use std::{iter, mem};
    use tokio::time;

    // exactly like `Swarm::new_ephemeral_tokio` but we can pass our own config
    fn new_ephemeral_swarm<T: swarm::NetworkBehaviour>(
        config: swarm::Config,
        behaviour: T,
    ) -> Swarm<T> {
        let identity = Keypair::generate_ed25519();
        let peer_id = PeerId::from(identity.public());

        let transport = MemoryTransport::default()
            .or_transport(libp2p::tcp::tokio::Transport::default())
            .upgrade(Version::V1)
            .authenticate(libp2p::plaintext::Config::new(&identity))
            .multiplex(libp2p::yamux::Config::default())
            .timeout(Duration::from_secs(20))
            .boxed();

        Swarm::new(transport, behaviour, peer_id, config)
    }

    async fn new_swarm_with_config(config: Config) -> (Swarm<Behaviour>, Database, DataProvider) {
        let data_provider = DataProvider::default();
        let db = Database::from_one(&MemDb::default());
        let behaviour = Behaviour::new(
            config,
            peer_score::Handle::new_test(),
            data_provider.clone_boxed(),
            Box::new(db.clone()),
        );
        let mut swarm = Swarm::new_ephemeral_tokio(move |_keypair| behaviour);
        swarm.listen().with_memory_addr_external().await;
        (swarm, db, data_provider)
    }

    async fn new_swarm() -> (Swarm<Behaviour>, Database, DataProvider) {
        new_swarm_with_config(Config::default()).await
    }

    #[tokio::test]
    async fn smoke() {
        init_logger();

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, bob_db, _data_provider) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();

        let hello_hash = bob_db.write_hash(b"hello");
        let world_hash = bob_db.write_hash(b"world");

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

        let request = alice_handle.request(Request::hashes([hello_hash, world_hash]));
        let request_id = request.request_id();

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
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes(
                [
                    (hello_hash, b"hello".to_vec()),
                    (world_hash, b"world".to_vec())
                ]
                .into()
            )
        )
    }

    #[tokio::test]
    async fn out_of_rounds() {
        init_logger();

        let alice_config = Config::default().with_max_rounds_per_request(1);
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;
        let alice_handle = alice.behaviour().handle();

        let mut bob = Swarm::new_ephemeral_tokio(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let request = alice_handle.request(Request::hashes([]));
        let request_id = request.request_id();

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
                    assert_eq!(request, InnerRequest::Hashes(HashesRequest::default()));
                    drop(channel);
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

        request.await.unwrap_err();
    }

    #[tokio::test(start_paused = true)]
    async fn timeout() {
        init_logger();

        let alice_config = Config::default().with_request_timeout(Duration::from_secs(3));
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;
        let alice_handle = alice.behaviour().handle();

        let mut bob = Swarm::new_ephemeral_tokio(|_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let request = alice_handle.request(Request::hashes([]));
        let request_id = request.request_id();

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
                    assert_eq!(request, InnerRequest::Hashes(HashesRequest::default()));
                    // just ignore request
                    mem::forget(channel);
                }
            }
        });

        time::advance(Config::default().request_timeout).await;

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestFailed {
                request_id,
                error: RequestFailure::Timeout,
            }
        );
        request.await.unwrap_err();
    }

    #[tokio::test]
    async fn excessive_data_stripped() {
        const DATA: [[u8; 1]; 3] = [*b"1", *b"2", *b"3"];

        init_logger();

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();

        let mut bob = Swarm::new_ephemeral_tokio(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let data_0 = ethexe_db::hash(&DATA[0]);
        let data_1 = ethexe_db::hash(&DATA[1]);
        let data_2 = ethexe_db::hash(&DATA[2]);

        let request = alice_handle.request(Request::hashes([data_0, data_1]));
        let request_id = request.request_id();

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
                    assert_eq!(
                        request,
                        InnerRequest::Hashes(HashesRequest([data_0, data_1].into()))
                    );
                    bob.behaviour_mut()
                        .send_response(
                            channel,
                            InnerHashesResponse(
                                [
                                    (data_0, DATA[0].to_vec()),
                                    (data_1, DATA[1].to_vec()),
                                    (data_2, DATA[2].to_vec()),
                                ]
                                .into(),
                            )
                            .into(),
                        )
                        .unwrap();
                }
            }
        });

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes([(data_0, DATA[0].to_vec()), (data_1, DATA[1].to_vec())].into())
        );
    }

    #[tokio::test]
    async fn request_response_type_mismatch() {
        init_logger();

        let alice_config = Config::default().with_max_rounds_per_request(1);
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;
        let alice_handle = alice.behaviour().handle();

        let mut bob = Swarm::new_ephemeral_tokio(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let request = alice_handle.request(Request::hashes([]));
        let request_id = request.request_id();

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
                    assert_eq!(request, InnerRequest::Hashes(HashesRequest::default()));
                    bob.behaviour_mut()
                        .send_response(channel, InnerProgramIdsResponse::default().into())
                        .unwrap();
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

        request.await.unwrap_err();
    }

    #[tokio::test]
    async fn request_completed_by_3_rounds() {
        init_logger();

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, bob_db, _data_provider) = new_swarm().await;
        let (mut charlie, charlie_db, _data_provider) = new_swarm().await;
        let (mut dave, dave_db, _data_provider) = new_swarm().await;

        alice.connect(&mut bob).await;
        alice.connect(&mut charlie).await;
        alice.connect(&mut dave).await;
        tokio::spawn(bob.loop_on_next());
        tokio::spawn(charlie.loop_on_next());
        tokio::spawn(dave.loop_on_next());

        let hello_hash = bob_db.write_hash(b"hello");
        let world_hash = charlie_db.write_hash(b"world");
        let mark_hash = dave_db.write_hash(b"!");

        let request = alice_handle.request(Request::hashes([hello_hash, world_hash, mark_hash]));
        let request_id = request.request_id();

        // first round
        let event = alice.next_behaviour_event().await;
        assert_matches!(
            event,
            Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::FromQueue, .. } if rid == request_id
        );
        // second round
        let event = alice.next_behaviour_event().await;
        assert_matches!(
            event,
            Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::PartialData, .. } if rid == request_id
        );
        // third round
        let event = alice.next_behaviour_event().await;
        assert_matches!(
            event,
            Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::PartialData, .. } if rid == request_id
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes(
                [
                    (hello_hash, b"hello".to_vec()),
                    (world_hash, b"world".to_vec()),
                    (mark_hash, b"!".to_vec()),
                ]
                .into()
            )
        );
    }

    #[tokio::test]
    async fn request_completed_after_new_peer() {
        init_logger();

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, bob_db, _data_provider) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();
        let (charlie, charlie_db, _data_provider) = new_swarm().await;
        let charlie_peer_id = *charlie.local_peer_id();
        let charlie_addr = charlie.external_addresses().next().cloned().unwrap();

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let hello_hash = bob_db.write_hash(b"hello");
        let world_hash = charlie_db.write_hash(b"world");

        let request = alice_handle.request(Request::hashes([hello_hash, world_hash]));
        let request_id = request.request_id();

        // first round
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

        tokio::spawn(charlie.loop_on_next());
        alice.dial_and_wait(charlie_addr).await;

        // second round
        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::NewRequestRound {
                request_id,
                peer_id: charlie_peer_id,
                reason: NewRequestRoundReason::FromQueue,
            }
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes(
                [
                    (hello_hash, b"hello".to_vec()),
                    (world_hash, b"world".to_vec())
                ]
                .into()
            )
        );
    }

    #[tokio::test]
    async fn unsupported_protocol_handled() {
        init_logger();

        let alice_config = Config::default().with_request_timeout(Duration::from_secs(2));
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;
        let alice_handle = alice.behaviour().handle();

        // idle connection timeout is lowered because `libp2p` uses `future_timer` inside,
        // so we cannot advance time like in tokio
        let mut bob = new_ephemeral_swarm(
            swarm::Config::with_tokio_executor()
                .with_idle_connection_timeout(Duration::from_secs(5)),
            InnerBehaviour::new([], request_response::Config::default()),
        );
        let bob_peer_id = *bob.local_peer_id();
        bob.connect(&mut alice).await;
        tokio::spawn(bob.loop_on_next());

        let request = alice_handle.request(Request::hashes([]));
        let request_id = request.request_id();

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

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestFailed {
                request_id,
                error: RequestFailure::Timeout
            }
        );

        let event = alice.next_swarm_event().await;
        assert_matches!(event, SwarmEvent::ConnectionClosed { peer_id, .. } if peer_id == bob_peer_id);
    }

    #[tokio::test]
    async fn simultaneous_responses_limit() {
        init_logger();

        let alice_config = Config::default().with_max_simultaneous_responses(0);
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;

        let (mut bob, _bob_db, _data_provider) = new_swarm().await;
        let bob_handle = bob.behaviour().handle();
        let bob_peer_id = *bob.local_peer_id();

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let fut = bob_handle.request(Request::hashes([]));
        mem::forget(fut);

        let event = alice.next_behaviour_event().await;
        assert_matches!(event, Event::IncomingRequestDropped { peer_id } if peer_id == bob_peer_id);
    }

    #[tokio::test(start_paused = true)]
    async fn retry() {
        init_logger();

        let alice_config = Config::default().with_max_rounds_per_request(1);
        let (mut alice, _alice_db, _data_provider) = new_swarm_with_config(alice_config).await;
        let alice_handle = alice.behaviour().handle();
        let mut bob = Swarm::new_ephemeral_tokio(move |_keypair| {
            InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            )
        });
        bob.connect(&mut alice).await;

        let request_key = ethexe_db::hash(b"test");
        let request = alice_handle.request(Request::hashes([request_key]));
        let request_id = request.request_id();

        // first round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::FromQueue, .. } if rid == request_id)
        );

        let bob_handle = tokio::spawn(async move {
            while let Some(event) = bob.next().await {
                if let Ok(request_response::Event::Message {
                    message:
                        Message::Request {
                            channel, request, ..
                        },
                    ..
                }) = event.try_into_behaviour_event()
                {
                    assert_eq!(
                        request,
                        InnerRequest::Hashes(HashesRequest([request_key].into()))
                    );
                    // just ignore request
                    mem::forget(channel);
                }
            }
        });

        time::advance(Config::default().request_timeout).await;

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::RequestFailed {
                request_id,
                error: RequestFailure::Timeout,
            }
        );
        let (error, retriable_request) = request.await.unwrap_err();
        assert_eq!(error, RequestFailure::Timeout);

        time::resume();

        bob_handle.abort();
        assert!(bob_handle.await.unwrap_err().is_cancelled());
        let (mut charlie, charlie_db, _data_provider) = new_swarm().await;
        alice.connect(&mut charlie).await;
        tokio::spawn(charlie.loop_on_next());

        let key = charlie_db.write_hash(b"test");
        assert_eq!(request_key, key);
        let request = alice_handle.retry(retriable_request);
        let request_id = request.request_id();

        // retry round
        let event = alice.next_behaviour_event().await;
        assert!(
            matches!(event, Event::NewRequestRound { request_id: rid, reason: NewRequestRoundReason::FromQueue, .. } if rid == request_id)
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes([(request_key, b"test".to_vec())].into())
        );
    }

    #[tokio::test]
    async fn external_data_provider() {
        init_logger();

        let (mut alice, _alice_db, alice_data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, _bob_db, _data_provider) = new_swarm().await;
        let (mut charlie, charlie_db, _data_provider) = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();

        let expected_response = fill_data_provider(alice_data_provider, charlie_db).await;

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let request = alice_handle.request(Request::program_ids(H256::zero(), 2));
        let request_id = request.request_id();

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
        assert_eq!(event, Event::PendingStateRequest { request_id });

        alice.connect(&mut charlie).await;
        tokio::spawn(charlie.loop_on_next());

        // `Event::NewRequestRound` skipped by `connect()` above

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(response, expected_response);
    }

    #[tokio::test]
    async fn request_cancelled() {
        let (mut alice, _db, _data_provider) = new_swarm().await;

        let request = alice.behaviour().handle().request(Request::hashes([]));
        let request_id = request.request_id();
        drop(request);

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestCancelled { request_id });
    }

    pub(crate) async fn fill_data_provider(
        // data provider of the first peer
        left_data_provider: DataProvider,
        // database of the second peer
        right_db: Database,
    ) -> Response {
        let program_ids: BTreeSet<ActorId> = [ActorId::new([1; 32]), ActorId::new([2; 32])].into();
        let code_ids = vec![CodeId::new([0xfe; 32]), CodeId::new([0xef; 32])];
        left_data_provider
            .set_programs_code_ids_at(program_ids.clone(), H256::zero(), code_ids.clone())
            .await;

        let mut announce_hash = HashOf::zero();
        right_db.mutate_block_meta(H256::zero(), |meta| {
            assert!(meta.announces.is_none());
            let announce = Announce::base(H256::zero(), HashOf::zero());
            announce_hash = announce.to_hash();
            meta.announces = Some([announce_hash].into());
        });

        right_db.set_announce_program_states(
            announce_hash,
            iter::zip(
                program_ids.clone(),
                iter::repeat_with(H256::random).map(|hash| StateHashWithQueueSize {
                    hash,
                    canonical_queue_size: 0,
                    injected_queue_size: 0,
                }),
            )
            .collect(),
        );

        Response::ProgramIds(iter::zip(program_ids, code_ids).collect())
    }
}
