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

use crate::{
    db_sync::{Multiaddr, PeerId},
    peer_score,
    utils::ParityScaleCodec,
};
use ethexe_common::injected::SignedInjectedTransaction;
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
use std::task::{Context, Poll, ready};

const STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/offchain-tx/", env!("CARGO_PKG_VERSION")));

#[derive(Debug, Encode, Decode)]
pub(crate) enum Request {
    InjectedTransaction(SignedInjectedTransaction),
}

#[derive(Debug, Encode, Decode)]
pub(crate) enum InjectedTransactionResponse {
    Accepted,
}

#[derive(Debug, Encode, Decode)]
pub(crate) enum Response {
    InjectedTransaction(InjectedTransactionResponse),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Event {
    NewInjectedTransaction(SignedInjectedTransaction),
}

type InnerBehaviour = request_response::Behaviour<ParityScaleCodec<Request, Response>>;

pub(crate) struct Behaviour {
    inner: InnerBehaviour,
    peer_score: peer_score::Handle,
}

impl Behaviour {
    pub fn new(peer_score: peer_score::Handle) -> Self {
        let inner = request_response::Behaviour::new(
            [(STREAM_PROTOCOL, ProtocolSupport::Full)],
            request_response::Config::default(),
        );
        Self { inner, peer_score }
    }

    #[allow(clippy::redundant_closure_call)]
    pub fn send_transaction(&mut self, data: SignedInjectedTransaction) {
        let peer: PeerId = (|| todo!("send to actual peer when validator discovery is ready"))();

        self.inner
            .send_request(&peer, Request::InjectedTransaction(data));
    }

    fn handle_inner_event(
        &mut self,
        event: request_response::Event<Request, Response>,
    ) -> Poll<Event> {
        match event {
            request_response::Event::Message {
                peer: _,
                connection_id: _,
                message:
                    Message::Request {
                        request_id: _,
                        request,
                        channel,
                    },
            } => {
                return match request {
                    Request::InjectedTransaction(transaction) => {
                        let _res = self.inner.send_response(
                            channel,
                            Response::InjectedTransaction(InjectedTransactionResponse::Accepted),
                        );
                        Poll::Ready(Event::NewInjectedTransaction(transaction))
                    }
                };
            }
            request_response::Event::Message {
                peer: _,
                connection_id: _,
                message:
                    Message::Response {
                        request_id: _,
                        response,
                    },
            } => {
                match response {
                    Response::InjectedTransaction(InjectedTransactionResponse::Accepted) => {}
                };
            }
            request_response::Event::OutboundFailure {
                peer,
                connection_id: _,
                request_id: _,
                error: OutboundFailure::UnsupportedProtocols,
            } => {
                log::debug!(
                    "request to {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                );
                self.peer_score.unsupported_protocol(peer);
            }
            request_response::Event::OutboundFailure { .. } => {}
            request_response::Event::InboundFailure {
                peer,
                connection_id: _,
                request_id: _,
                error: InboundFailure::UnsupportedProtocols,
            } => {
                log::debug!(
                    "request from {peer} failed because it doesn't support {STREAM_PROTOCOL} protocol"
                );
                self.peer_score.unsupported_protocol(peer);
            }
            request_response::Event::InboundFailure { .. } => {}
            request_response::Event::ResponseSent { .. } => {}
        }

        Poll::Pending
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = <InnerBehaviour as NetworkBehaviour>::ConnectionHandler;
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
