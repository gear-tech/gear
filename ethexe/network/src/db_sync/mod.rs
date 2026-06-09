// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Peer-to-peer database synchronization for `ethexe`.
//!
//! The protocol is built on libp2p request/response and is used to fetch data
//! that can be revalidated locally: raw CAS blobs, program-to-code mappings,
//! and valid code sets. Requests are driven through [`Handle`], while the
//! behaviour internally retries across peers, enforces a per-request
//! timeout, and limits concurrent inbound responses.

mod requests;
mod responses;

use crate::{db_sync::requests::OngoingRequests, peer_score, utils::AlternateCollectionFmt};
pub(crate) use crate::{
    db_sync::{requests::RetriableRequest, responses::OngoingResponses},
    export::{Multiaddr, PeerId},
    utils::ParityScaleCodec,
};
use async_trait::async_trait;
use ethexe_common::{
    db::{
        BlockMetaStorageRO, CodesStorageRO, ConfigStorageRO, GlobalsStorageRO, HashStorageRO,
        MbStorageRO,
    },
    gear::CodeState,
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

const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/ethexe/db-sync/1.0.0");

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_network_db_sync")]
struct Metrics {
    /// Number of either active or pending requests
    ongoing_requests: metrics::Gauge,
    /// Number of incoming dropped requests
    incoming_dropped_requests: metrics::Counter,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, derive_more::Display)]
pub enum RequestFailure {
    /// Request had been processing for too long
    #[display("Request had been processing for too long")]
    Timeout,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Event {
    /// Request is in a pending state because there are no peers
    NoPeers {
        /// The ID of request
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
    pub request_timeout: Duration,
    pub max_simultaneous_responses: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(100),
            max_simultaneous_responses: 10,
        }
    }
}

#[cfg(test)] // used only in tests yet
impl Config {
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

/// An asynchronous provider of external blockchain data required for response validation.
#[async_trait]
pub trait ExternalDataProvider: Send + Sync {
    /// Clone the provider as a trait object.
    fn clone_boxed(&self) -> Box<dyn ExternalDataProvider>;

    /// Resolve program IDs to code IDs at the given block.
    async fn programs_code_ids_at(
        self: Box<Self>,
        program_ids: BTreeSet<ActorId>,
        block: H256,
    ) -> anyhow::Result<Vec<CodeId>>;

    /// Resolve code IDs to code states at the given block.
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

/// Request to fetch the program-to-code mapping visible at a specific block.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ProgramIdsRequest {
    pub at: H256,
    pub expected_count: u64,
}

/// Request to fetch the current set of valid codes and verify the response
/// using [`ExternalDataProvider`] at a specific block.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ValidCodesRequest {
    pub at: H256,
    pub validated_count: u64,
}

/// High-level db-sync request types supported by the network.
#[derive(Debug, Clone, Eq, PartialEq, derive_more::From)]
pub enum Request {
    /// Fetch raw CAS blobs by hash.
    Hashes(HashesRequest),
    /// Fetch the program-to-code mapping for a block.
    ProgramIds(ProgramIdsRequest),
    /// Fetch the node's locally stored set of valid code IDs.
    ValidCodes(ValidCodesRequest),
}

impl Request {
    /// Build a request for a set of CAS hashes.
    pub fn hashes(request: impl Into<BTreeSet<H256>>) -> Self {
        Self::Hashes(HashesRequest(request.into()))
    }

    /// Build a request for program-to-code mappings at `at`.
    pub fn program_ids(at: H256, expected_count: u64) -> Self {
        Self::ProgramIds(ProgramIdsRequest { at, expected_count })
    }

    /// Build a request for the valid code set, using `at` only for response
    /// verification.
    pub fn valid_codes(at: H256, validated_count: u64) -> Self {
        Self::ValidCodes(ValidCodesRequest {
            at,
            validated_count,
        })
    }
}

/// Successful db-sync responses returned to callers.
#[derive(derive_more::Debug, Clone, Eq, PartialEq, derive_more::From, derive_more::Unwrap)]
pub enum Response {
    /// Raw CAS blobs keyed by hash.
    Hashes(#[debug("{:?}", AlternateCollectionFmt::map(_0, "entries"))] BTreeMap<H256, Vec<u8>>),
    /// Program-to-code mapping reconstructed for a block.
    ProgramIds(
        #[debug("{:?}", AlternateCollectionFmt::map(_0, "programs"))] BTreeMap<ActorId, CodeId>,
    ),
    /// Set of valid code IDs known at a block.
    ValidCodes(#[debug("{:?}", AlternateCollectionFmt::set(_0, "codes"))] BTreeSet<CodeId>),
}

/// Result delivered by [`HandleFuture`].
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

/// Future returned by [`Handle::request`] and [`Handle::retry`].
pub struct HandleFuture {
    request_id: RequestId,
    rx: oneshot::Receiver<HandleResult>,
}

impl HandleFuture {
    /// Returns the identifier assigned to this request.
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

    /// Enqueue a new request.
    pub fn request(&self, request: Request) -> HandleFuture {
        self.send(HandleAction::Request(RequestId::next(), request))
    }

    /// Re-enqueue a retriable request returned by a previous failure.
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
}

#[derive(Debug, Clone, Default, Eq, PartialEq, Encode, Decode)]
pub(crate) struct InnerHashesResponse(BTreeMap<H256, Vec<u8>>);

#[derive(Debug, Default, Eq, PartialEq, Encode, Decode)]
pub(crate) struct InnerProgramIdsResponse(BTreeSet<ActorId>);

/// Network-only type to be encoded-decoded and sent over the network
#[derive(Debug, Eq, PartialEq, derive_more::From, derive_more::Unwrap, Encode, Decode)]
pub(crate) enum InnerResponse {
    Hashes(InnerHashesResponse),
    ProgramIds(InnerProgramIdsResponse),
    ValidCodes(BTreeSet<CodeId>),
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<InnerRequest, InnerResponse>>;

#[auto_impl::auto_impl(&, Box)]
pub trait DbSyncDatabase:
    Send
    + HashStorageRO
    + BlockMetaStorageRO
    + CodesStorageRO
    + ConfigStorageRO
    + GlobalsStorageRO
    + MbStorageRO
{
    /// Clone the database as a trait object.
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
    ongoing_requests: OngoingRequests,
    ongoing_responses: OngoingResponses,
    metrics: Metrics,
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
            ongoing_requests: OngoingRequests::new(
                &config,
                peer_score_handle,
                external_data_provider,
            ),
            ongoing_responses: OngoingResponses::new(db, &config),
            metrics: Metrics::default(),
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
                    self.metrics.incoming_dropped_requests.increment(1);
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

        if let Poll::Ready(request_event) =
            self.ongoing_requests
                .poll(cx, &mut self.inner, &self.metrics)
        {
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{tests::DataProvider, utils::tests::init_logger};
    use assert_matches::assert_matches;
    use ethexe_db::Database;
    use libp2p::{
        Swarm, Transport,
        core::{transport::MemoryTransport, upgrade::Version},
        futures::StreamExt,
        identity::Keypair,
        swarm,
        swarm::SwarmEvent,
    };
    use libp2p_swarm_test::SwarmExt;
    use std::mem;
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
        let db = Database::memory();
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

        let hello_hash = bob_db.cas().write(b"hello");
        let world_hash = bob_db.cas().write(b"world");

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
    async fn truncated_hashes_response_completed_from_same_peer() {
        const DATA_LEN: usize = 6 * 1024 * 1024;

        init_logger();

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, bob_db, _data_provider) = new_swarm().await;

        let data_0 = vec![0; DATA_LEN];
        let data_1 = vec![1; DATA_LEN];
        let hash_0 = bob_db.cas().write(&data_0);
        let hash_1 = bob_db.cas().write(&data_1);

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let request = alice_handle.request(Request::hashes([hash_0, hash_1]));
        let request_id = request.request_id();

        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes([(hash_0, data_0), (hash_1, data_1)].into())
        );
    }

    #[tokio::test]
    async fn request_response_type_mismatch() {
        init_logger();

        let alice_config = Config::default().with_request_timeout(Duration::ZERO);
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
                error: RequestFailure::Timeout,
            }
        );

        request.await.unwrap_err();
    }

    #[tokio::test]
    async fn request_completed_with_3_peers() {
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

        let hello_hash = bob_db.cas().write(b"hello");
        let world_hash = charlie_db.cas().write(b"world");
        let mark_hash = dave_db.cas().write(b"!");

        let request = alice_handle.request(Request::hashes([hello_hash, world_hash, mark_hash]));
        let request_id = request.request_id();

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
        let (charlie, charlie_db, _data_provider) = new_swarm().await;
        let charlie_addr = charlie.external_addresses().next().cloned().unwrap();

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let hello_hash = bob_db.cas().write(b"hello");
        let world_hash = charlie_db.cas().write(b"world");

        let request = alice_handle.request(Request::hashes([hello_hash, world_hash]));
        let request_id = request.request_id();

        // first attempt
        let event = alice.next_behaviour_event().now_or_never();
        assert_eq!(event, None);

        tokio::spawn(charlie.loop_on_next());
        alice.dial_and_wait(charlie_addr).await;

        // second attempt
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

    #[tokio::test(start_paused = true)]
    async fn unsupported_protocol_handled() {
        const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

        init_logger();

        let alice_config = Config::default().with_request_timeout(REQUEST_TIMEOUT);
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

        // activate timer
        let event = alice.next_behaviour_event().now_or_never();
        assert_eq!(event, None);

        time::advance(REQUEST_TIMEOUT).await;

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

        let (mut alice, _alice_db, _data_provider) = new_swarm().await;
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

        // first attempt
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

        let key = charlie_db.cas().write(b"test");
        assert_eq!(request_key, key);
        let request = alice_handle.retry(retriable_request);
        let request_id = request.request_id();

        // retry attempt
        let event = alice.next_behaviour_event().await;
        assert_eq!(event, Event::RequestSucceed { request_id });

        let response = request.await.unwrap();
        assert_eq!(
            response,
            Response::Hashes([(request_key, b"test".to_vec())].into())
        );
    }

    #[tokio::test]
    #[ignore = "test setup populates the requester's data provider rather than the responder's; \
                needs a real responder-side fixture"]
    async fn external_data_provider() {
        init_logger();

        let (mut alice, _alice_db, alice_data_provider) = new_swarm().await;
        let alice_handle = alice.behaviour().handle();
        let (mut bob, _bob_db, _data_provider) = new_swarm().await;

        let expected_response = fill_data_provider(alice_data_provider).await;

        alice.connect(&mut bob).await;
        tokio::spawn(bob.loop_on_next());

        let request = alice_handle.request(Request::program_ids(H256::zero(), 2));
        let request_id = request.request_id();

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

    pub(crate) async fn fill_data_provider(left_data_provider: DataProvider) -> Response {
        let program_ids: BTreeSet<ActorId> = [ActorId::new([1; 32]), ActorId::new([2; 32])].into();
        let code_ids = vec![CodeId::new([0xfe; 32]), CodeId::new([0xef; 32])];
        left_data_provider
            .set_programs_code_ids_at(program_ids.clone(), H256::zero(), code_ids.clone())
            .await;
        Response::ProgramIds(std::iter::zip(program_ids, code_ids).collect())
    }
}
