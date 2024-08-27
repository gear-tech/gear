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

use crate::export::{Multiaddr, PeerId};
use libp2p::{
    allow_block_list,
    core::{transport::PortUse, Endpoint},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use std::{
    collections::HashMap,
    mem,
    task::{Context, Poll},
};
use tokio::sync::mpsc;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum ScoreEvent {
    UnsupportedProtocol,
    ExcessiveData,
}

impl ScoreEvent {
    fn abs_diff(self) -> u8 {
        match self {
            ScoreEvent::UnsupportedProtocol => u8::MAX,
            ScoreEvent::ExcessiveData => u8::MAX,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PeerScoreHandle(mpsc::UnboundedSender<(PeerId, ScoreEvent)>);

impl PeerScoreHandle {
    pub fn new_test() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        mem::forget(rx);
        Self(tx)
    }

    pub fn unsupported_protocol(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreEvent::UnsupportedProtocol));
    }

    pub fn excessive_data(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreEvent::ExcessiveData));
    }
}

type BlockListBehaviour = allow_block_list::Behaviour<allow_block_list::BlockedPeers>;

pub(crate) struct Behaviour {
    block_list: BlockListBehaviour,
    handle: PeerScoreHandle,
    rx: mpsc::UnboundedReceiver<(PeerId, ScoreEvent)>,
    score: HashMap<PeerId, u8>,
}

impl Behaviour {
    pub(crate) fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = PeerScoreHandle(tx);
        Self {
            block_list: BlockListBehaviour::default(),
            handle,
            rx,
            score: HashMap::new(),
        }
    }

    pub(crate) fn handle(&self) -> PeerScoreHandle {
        self.handle.clone()
    }

    fn on_score_event(&mut self, peer_id: PeerId, event: ScoreEvent) {
        let peer_score = self.score.entry(peer_id).or_insert(u8::MAX);
        *peer_score = peer_score.saturating_sub(event.abs_diff());

        if *peer_score == u8::MIN {
            let removed = self.score.remove(&peer_id);
            debug_assert!(removed.is_some());
            self.block_list.block_peer(peer_id);
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<BlockListBehaviour>;
    type ToSwarm = void::Void;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.block_list
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.block_list.handle_established_inbound_connection(
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
        self.block_list.handle_pending_outbound_connection(
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
        self.block_list.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.block_list.on_swarm_event(event);
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.block_list
            .on_connection_handler_event(peer_id, connection_id, event);
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Poll::Ready(to_swarm) = self.block_list.poll(cx) {
            match to_swarm {
                ToSwarm::GenerateEvent(event) => void::unreachable(event),
                to_swarm => return Poll::Ready(to_swarm),
            }
        }

        if let Poll::Ready(Some((peer_id, event))) = self.rx.poll_recv(cx) {
            self.on_score_event(peer_id, event);
        }

        Poll::Pending
    }
}
