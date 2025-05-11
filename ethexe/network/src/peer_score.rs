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
    task::{Context, Poll},
};
use tokio::sync::mpsc;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScoreChangedReason {
    UnsupportedProtocol,
    ExcessiveData,
    InvalidData,
}

impl ScoreChangedReason {
    fn to_u8(self, config: &Config) -> u8 {
        match self {
            ScoreChangedReason::UnsupportedProtocol => config.unsupported_protocol,
            ScoreChangedReason::ExcessiveData => config.excessive_data,
            ScoreChangedReason::InvalidData => config.invalid_data,
        }
    }
}

/// Handle to report peer actions
#[derive(Debug, Clone)]
pub struct Handle(mpsc::UnboundedSender<(PeerId, ScoreChangedReason)>);

impl Handle {
    #[cfg(test)]
    pub fn new_test() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        std::mem::forget(rx);
        Self(tx)
    }

    pub fn unsupported_protocol(&self, peer_id: PeerId) {
        let _res = self
            .0
            .send((peer_id, ScoreChangedReason::UnsupportedProtocol));
    }

    pub fn excessive_data(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreChangedReason::ExcessiveData));
    }

    pub fn invalid_data(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreChangedReason::InvalidData));
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Event {
    /// Peer got so low score it's blocked now
    PeerBlocked {
        /// Peer we blocked
        peer_id: PeerId,
        /// The last reason changed peer score
        last_reason: ScoreChangedReason,
    },
    /// Peer score has been changed
    ScoreChanged {
        /// Peer whose score has been changed
        peer_id: PeerId,
        /// Reason why score is changed
        reason: ScoreChangedReason,
        /// Current peer score
        score: u8,
    },
}

/// Behaviour config.
///
/// All values represented by number that will be subtracted from peer score.
pub(crate) struct Config {
    unsupported_protocol: u8,
    excessive_data: u8,
    invalid_data: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            unsupported_protocol: u8::MAX,
            excessive_data: u8::MAX,
            invalid_data: u8::MAX,
        }
    }
}

#[cfg(test)] // used only in tests yet
impl Config {
    #[allow(dead_code)] // not used anywhere yet
    pub(crate) fn with_unsupported_protocol(mut self, value: u8) -> Self {
        self.unsupported_protocol = value;
        self
    }

    pub(crate) fn with_excessive_data(mut self, value: u8) -> Self {
        self.excessive_data = value;
        self
    }
}

type BlockListBehaviour = allow_block_list::Behaviour<allow_block_list::BlockedPeers>;

pub(crate) struct Behaviour {
    config: Config,
    block_list: BlockListBehaviour,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(PeerId, ScoreChangedReason)>,
    score: HashMap<PeerId, u8>,
}

impl Behaviour {
    pub(crate) fn new(config: Config) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Handle(tx);
        Self {
            config,
            block_list: BlockListBehaviour::default(),
            handle,
            rx,
            score: HashMap::new(),
        }
    }

    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn on_score_event(&mut self, peer_id: PeerId, reason: ScoreChangedReason) -> Event {
        let peer_score = self.score.entry(peer_id).or_insert(u8::MAX);
        *peer_score = peer_score.saturating_sub(reason.to_u8(&self.config));

        if *peer_score == u8::MIN {
            let removed = self.score.remove(&peer_id);
            debug_assert!(removed.is_some());
            self.block_list.block_peer(peer_id);

            Event::PeerBlocked {
                peer_id,
                last_reason: reason,
            }
        } else {
            Event::ScoreChanged {
                peer_id,
                reason,
                score: *peer_score,
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = THandler<BlockListBehaviour>;
    type ToSwarm = Event;

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
            return Poll::Ready(to_swarm.map_out(|infallible| match infallible {}));
        }

        if let Poll::Ready(Some((peer_id, reason))) = self.rx.poll_recv(cx) {
            return Poll::Ready(ToSwarm::GenerateEvent(self.on_score_event(peer_id, reason)));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::{swarm::SwarmEvent, Swarm};
    use libp2p_swarm_test::SwarmExt;

    async fn new_swarm_with_config(config: Config) -> Swarm<Behaviour> {
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| Behaviour::new(config));
        swarm.listen().with_memory_addr_external().await;
        swarm
    }

    async fn new_swarm() -> Swarm<Behaviour> {
        new_swarm_with_config(Config::default()).await
    }

    #[tokio::test]
    async fn peer_blocked() {
        const EXCESSIVE_DATA_ABS_DIFF: u8 = u8::MAX / 3;

        let alice_config = Config::default().with_excessive_data(EXCESSIVE_DATA_ABS_DIFF);
        let mut alice = new_swarm_with_config(alice_config).await;
        let mut chad = new_swarm().await;
        let alice_peer_id = *alice.local_peer_id();
        let chad_peer_id = *chad.local_peer_id();
        alice.connect(&mut chad).await;

        let handle = alice.behaviour_mut().handle();
        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::ScoreChanged {
                peer_id: chad_peer_id,
                reason: ScoreChangedReason::ExcessiveData,
                score: u8::MAX - EXCESSIVE_DATA_ABS_DIFF,
            }
        );

        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::ScoreChanged {
                peer_id: chad_peer_id,
                reason: ScoreChangedReason::ExcessiveData,
                score: u8::MAX - 2 * EXCESSIVE_DATA_ABS_DIFF,
            }
        );

        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::PeerBlocked {
                peer_id: chad_peer_id,
                last_reason: ScoreChangedReason::ExcessiveData
            }
        );

        let event = chad.next_swarm_event().await;
        assert!(
            matches!(
                event,
                SwarmEvent::ConnectionClosed {
                    peer_id,
                    num_established: 0,
                    ..
                } if peer_id == alice_peer_id
            ),
            "{event:?}"
        );
    }
}
