// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Request/response synchronization for MB-indexed database rows.

use crate::utils::ParityScaleCodec;
use ethexe_common::{
    ProgramStates, Schedule,
    db::{CompactMb, MbMeta, MbStorageRO},
    gear::StateTransition,
};
use ethexe_db::Database;
use gprimitives::H256;
use libp2p::{
    Multiaddr, PeerId, StreamProtocol,
    core::{Endpoint, transport::PortUse},
    request_response::{self, Message, ProtocolSupport},
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::{HashMap, HashSet},
    task::{Context, Poll},
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};

const STREAM_PROTOCOL: StreamProtocol = StreamProtocol::new("/ethexe/mb-sync/1.0.0");

#[derive(Debug, Copy, Clone, Eq, PartialEq, Encode, Decode)]
pub enum Request {
    CompactMb(H256),
    ProgramStates(H256),
    Outcome(H256),
    Schedule(H256),
    Meta(H256),
}

#[derive(Debug, Clone, Eq, PartialEq, Encode, Decode)]
pub enum Response {
    CompactMb(Option<CompactMb>),
    ProgramStates(Option<ProgramStates>),
    Outcome(Option<Vec<StateTransition>>),
    Schedule(Option<Schedule>),
    Meta(MbMeta),
}

impl Response {
    fn is_missing(&self) -> bool {
        match self {
            Self::CompactMb(value) => value.is_none(),
            Self::ProgramStates(value) => value.is_none(),
            Self::Outcome(value) => value.is_none(),
            Self::Schedule(value) => value.is_none(),
            Self::Meta(_) => false,
        }
    }
}

#[derive(Clone)]
pub struct Handle(mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>);

impl Handle {
    pub async fn request(&self, request: Request) -> Response {
        let (tx, rx) = oneshot::channel();
        self.0.send((request, tx)).expect("channel should be open");
        rx.await.expect("channel should be open")
    }
}

#[auto_impl::auto_impl(&, Box)]
pub trait MbSyncDatabase: Send + Sync + MbStorageRO {
    fn clone_boxed(&self) -> Box<dyn MbSyncDatabase>;
}

impl MbSyncDatabase for Database {
    fn clone_boxed(&self) -> Box<dyn MbSyncDatabase> {
        Box::new(self.clone())
    }
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

struct PendingRequest {
    request: Request,
    channel: oneshot::Sender<Response>,
    active: Option<request_response::OutboundRequestId>,
    tried_peers: HashSet<PeerId>,
}

struct OngoingResponse {
    peer: PeerId,
    channel: request_response::ResponseChannel<Response>,
    response: Response,
}

pub struct Behaviour {
    inner: InnerBehaviour,
    db: Box<dyn MbSyncDatabase>,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(Request, oneshot::Sender<Response>)>,
    peers: HashSet<PeerId>,
    pending: HashMap<u64, PendingRequest>,
    active: HashMap<request_response::OutboundRequestId, u64>,
    next_request_id: u64,
    responses: JoinSet<OngoingResponse>,
}

impl Behaviour {
    pub fn new(db: Box<dyn MbSyncDatabase>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        Self {
            inner: InnerBehaviour::new(
                [(STREAM_PROTOCOL, ProtocolSupport::Full)],
                request_response::Config::default(),
            ),
            db,
            handle: Handle(tx),
            rx,
            peers: HashSet::new(),
            pending: HashMap::new(),
            active: HashMap::new(),
            next_request_id: 0,
            responses: JoinSet::new(),
        }
    }

    pub fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn response_from_db(db: Box<dyn MbSyncDatabase>, request: Request) -> Response {
        match request {
            Request::CompactMb(mb_hash) => Response::CompactMb(db.mb_compact_block(mb_hash)),
            Request::ProgramStates(mb_hash) => {
                Response::ProgramStates(db.mb_program_states(mb_hash))
            }
            Request::Outcome(mb_hash) => Response::Outcome(db.mb_outcome(mb_hash)),
            Request::Schedule(mb_hash) => Response::Schedule(db.mb_schedule(mb_hash)),
            Request::Meta(mb_hash) => Response::Meta(db.mb_meta(mb_hash)),
        }
    }

    fn next_peer(peers: &HashSet<PeerId>, tried_peers: &mut HashSet<PeerId>) -> Option<PeerId> {
        if peers.is_empty() {
            return None;
        }

        if peers.len() == tried_peers.len() {
            tried_peers.clear();
        }

        let peer = peers.difference(tried_peers).next().copied()?;
        tried_peers.insert(peer);
        Some(peer)
    }

    fn poll_pending_requests(&mut self) {
        while let Ok((request, channel)) = self.rx.try_recv() {
            let request_id = self.next_request_id;
            self.next_request_id += 1;

            self.pending.insert(
                request_id,
                PendingRequest {
                    request,
                    channel,
                    active: None,
                    tried_peers: HashSet::new(),
                },
            );
        }

        for request_id in self.pending.keys().copied().collect::<Vec<_>>() {
            let Some(pending) = self.pending.get_mut(&request_id) else {
                continue;
            };

            if pending.active.is_some() {
                continue;
            }

            let Some(peer) = Self::next_peer(&self.peers, &mut pending.tried_peers) else {
                continue;
            };

            let outbound_request_id = self.inner.send_request(&peer, pending.request);
            pending.active = Some(outbound_request_id);
            self.active.insert(outbound_request_id, request_id);
        }
    }

    fn handle_peer_response(
        &mut self,
        request_id: request_response::OutboundRequestId,
        response: Response,
    ) {
        let Some(local_request_id) = self.active.remove(&request_id) else {
            return;
        };

        if response.is_missing() {
            if let Some(pending) = self.pending.get_mut(&local_request_id) {
                log::trace!(
                    "MB sync peer returned no data for {:?}, trying another peer",
                    pending.request
                );
                pending.active = None;
            }
        } else if let Some(pending) = self.pending.remove(&local_request_id) {
            let _ = pending.channel.send(response);
        }
    }

    fn handle_peer_failure(&mut self, request_id: request_response::OutboundRequestId) {
        let Some(local_request_id) = self.active.remove(&request_id) else {
            return;
        };

        if let Some(pending) = self.pending.get_mut(&local_request_id) {
            pending.active = None;
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<InnerBehaviour>;
    type ToSwarm = ();

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
        self.peers.insert(peer);
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
        self.peers.insert(peer);
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            remaining_established,
            ..
        }) = &event
            && *remaining_established == 0
        {
            self.peers.remove(peer_id);
        }

        self.inner.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.poll_pending_requests();

        if let Poll::Ready(Some(response)) = self.responses.poll_join_next(cx) {
            let OngoingResponse {
                peer,
                channel,
                response,
            } = response.expect("database read panicked");
            let _ = self.inner.send_response(channel, response);
            log::trace!("sent MB sync response to {peer}");
        }

        if let Poll::Ready(to_swarm) = self.inner.poll(cx) {
            return match to_swarm {
                ToSwarm::GenerateEvent(event) => {
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
                            let db = self.db.clone_boxed();
                            self.responses.spawn_blocking(move || OngoingResponse {
                                peer,
                                channel,
                                response: Self::response_from_db(db, request),
                            });
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
                            self.handle_peer_response(request_id, response);
                        }
                        request_response::Event::OutboundFailure {
                            peer: _,
                            connection_id: _,
                            request_id,
                            error,
                        } => {
                            log::trace!("MB sync outbound request {request_id:?} failed: {error}");
                            self.handle_peer_failure(request_id);
                        }
                        request_response::Event::InboundFailure { .. }
                        | request_response::Event::ResponseSent { .. } => {}
                    }

                    Poll::Pending
                }
                to_swarm => {
                    Poll::Ready(to_swarm.map_out(|_event| {
                        unreachable!("`ToSwarm::GenerateEvent` is handled above")
                    }))
                }
            };
        }

        Poll::Pending
    }
}
