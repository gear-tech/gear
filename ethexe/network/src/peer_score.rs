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
    core::{Endpoint, transport::PortUse},
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use std::{
    collections::{HashMap, VecDeque},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::mpsc,
    time,
    time::{Instant, Interval},
};

const PEER_FORGET_TIME: Duration = Duration::from_hours(1);

#[derive(Debug, Copy, Clone, Eq, PartialEq, derive_more::From)]
pub(crate) enum ScoreChangeReason {
    Decrease(ScoreDecreaseReason),
    Increase,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScoreDecreaseReason {
    UnsupportedProtocol,
    ExcessiveData,
    InvalidData,
}

impl ScoreDecreaseReason {
    fn to_u8(self, config: &Config) -> u8 {
        match self {
            ScoreDecreaseReason::UnsupportedProtocol => config.unsupported_protocol,
            ScoreDecreaseReason::ExcessiveData => config.excessive_data,
            ScoreDecreaseReason::InvalidData => config.invalid_data,
        }
    }
}

/// Handle to report peer actions
#[derive(Debug, Clone)]
pub struct Handle(mpsc::UnboundedSender<(PeerId, ScoreDecreaseReason)>);

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
            .send((peer_id, ScoreDecreaseReason::UnsupportedProtocol));
    }

    pub fn excessive_data(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreDecreaseReason::ExcessiveData));
    }

    pub fn invalid_data(&self, peer_id: PeerId) {
        let _res = self.0.send((peer_id, ScoreDecreaseReason::InvalidData));
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Event {
    /// Peer got so low score it's blocked now
    PeerBlocked {
        /// Peer we blocked
        peer_id: PeerId,
        /// The last reason changed peer score
        last_reason: ScoreDecreaseReason,
    },
    /// Peer has been unblocked because of healing
    PeerUnblocked {
        /// Peer we blocked
        peer_id: PeerId,
    },
    /// Peer score has been changed
    ScoreChanged {
        /// Peer whose score has been changed
        peer_id: PeerId,
        /// Reason why score is changed
        reason: ScoreChangeReason,
        /// Current peer score
        score: u8,
    },
}

/// Behaviour config.
///
/// All values represented by number that will be subtracted from peer score.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    unsupported_protocol: u8,
    excessive_data: u8,
    invalid_data: u8,
    healing_increment: u8,
    driver_time: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            unsupported_protocol: u8::MAX,
            excessive_data: u8::MAX / 5,
            invalid_data: u8::MAX / 3,
            healing_increment: u8::MAX / 17,
            driver_time: Duration::from_secs(1),
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

enum ScoreEntry {
    Normal { score: u8 },
    Banned { at: Instant },
}

type BlockListBehaviour = allow_block_list::Behaviour<allow_block_list::BlockedPeers>;

pub(crate) struct Behaviour {
    pending_events: VecDeque<Event>,
    config: Config,
    block_list: BlockListBehaviour,
    handle: Handle,
    rx: mpsc::UnboundedReceiver<(PeerId, ScoreDecreaseReason)>,
    peers: HashMap<PeerId, ScoreEntry>,
    driver: Interval,
}

impl Behaviour {
    pub(crate) fn new(config: Config) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let handle = Handle(tx);
        Self {
            pending_events: VecDeque::new(),
            driver: time::interval(config.driver_time),
            config,
            block_list: BlockListBehaviour::default(),
            handle,
            rx,
            peers: HashMap::new(),
        }
    }

    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }

    fn on_driver_tick(&mut self) {
        let now = Instant::now();

        self.peers.retain(|&peer_id, entry| {
            let score = match entry {
                ScoreEntry::Normal { score } => score,
                ScoreEntry::Banned { at } => {
                    if *at + PEER_FORGET_TIME <= now {
                        return false;
                    }

                    *entry = ScoreEntry::Normal { score: u8::MIN };
                    let ScoreEntry::Normal { score } = entry else {
                        unreachable!()
                    };

                    self.block_list.unblock_peer(peer_id);
                    self.pending_events
                        .push_back(Event::PeerUnblocked { peer_id });

                    score
                }
            };

            if let Some(new_score) = score.checked_add(self.config.healing_increment) {
                *score = new_score;

                self.pending_events.push_back(Event::ScoreChanged {
                    peer_id,
                    reason: ScoreChangeReason::Increase,
                    score: *score,
                });
            }

            true
        })
    }

    fn on_score_decrease(&mut self, peer_id: PeerId, reason: ScoreDecreaseReason) {
        let entry = self
            .peers
            .entry(peer_id)
            .or_insert(ScoreEntry::Normal { score: u8::MAX });

        match entry {
            ScoreEntry::Normal { score: peer_score } => {
                *peer_score = peer_score.saturating_sub(reason.to_u8(&self.config));

                self.pending_events.push_back(Event::ScoreChanged {
                    peer_id,
                    reason: reason.into(),
                    score: *peer_score,
                });

                if *peer_score == u8::MIN {
                    *entry = ScoreEntry::Banned { at: Instant::now() };

                    self.block_list.block_peer(peer_id);
                    self.pending_events.push_back(Event::PeerBlocked {
                        peer_id,
                        last_reason: reason,
                    });
                }
            }
            ScoreEntry::Banned { at: _ } => {
                // nothing to decrease, peer is already banned
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
        if let Some(event) = self.pending_events.pop_front() {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        if let Poll::Ready(_instant) = self.driver.poll_tick(cx) {
            self.on_driver_tick();

            // return event produced by `on_driver_tick` immediately instead of waking
            if let Some(event) = self.pending_events.pop_front() {
                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
        }

        if let Poll::Ready(to_swarm) = self.block_list.poll(cx) {
            return Poll::Ready(to_swarm.map_out(|infallible| match infallible {}));
        }

        if let Poll::Ready(Some((peer_id, reason))) = self.rx.poll_recv(cx) {
            self.on_score_decrease(peer_id, reason);

            // return event produced by `on_score_decrease` immediately instead of waking
            if let Some(event) = self.pending_events.pop_front() {
                return Poll::Ready(ToSwarm::GenerateEvent(event));
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::{Swarm, swarm::SwarmEvent};
    use libp2p_swarm_test::SwarmExt;
    use tokio::time;

    async fn new_swarm_with_config(config: Config) -> Swarm<Behaviour> {
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| Behaviour::new(config));
        swarm.listen().with_memory_addr_external().await;
        swarm
    }

    async fn new_swarm() -> Swarm<Behaviour> {
        new_swarm_with_config(Config::default()).await
    }

    #[tokio::test(start_paused = true)]
    async fn peer_blocked() {
        const EXCESSIVE_DATA_ABS_DIFF: u8 = u8::MAX / 3;

        let alice_config = Config::default().with_excessive_data(EXCESSIVE_DATA_ABS_DIFF);
        let mut alice = new_swarm_with_config(alice_config.clone()).await;
        let mut chad = new_swarm().await;
        let chad_peer_id = *chad.local_peer_id();
        alice.connect(&mut chad).await;
        tokio::spawn(chad.loop_on_next());

        let handle = alice.behaviour_mut().handle();
        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::ScoreChanged {
                peer_id: chad_peer_id,
                reason: ScoreDecreaseReason::ExcessiveData.into(),
                score: u8::MAX - EXCESSIVE_DATA_ABS_DIFF,
            }
        );

        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::ScoreChanged {
                peer_id: chad_peer_id,
                reason: ScoreDecreaseReason::ExcessiveData.into(),
                score: u8::MAX - 2 * EXCESSIVE_DATA_ABS_DIFF,
            }
        );

        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::ScoreChanged {
                peer_id: chad_peer_id,
                reason: ScoreDecreaseReason::ExcessiveData.into(),
                score: 0,
            }
        );

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::PeerBlocked {
                peer_id: chad_peer_id,
                last_reason: ScoreDecreaseReason::ExcessiveData
            }
        );

        let event = alice.next_swarm_event().await;
        assert!(
            matches!(
                event,
                SwarmEvent::ConnectionClosed {
                    peer_id,
                    num_established: 0,
                    ..
                } if peer_id == chad_peer_id
            ),
            "{event:?}"
        );

        time::advance(alice_config.driver_time).await;

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::PeerUnblocked {
                peer_id: chad_peer_id,
            }
        );

        for i in 0..u8::MAX / alice_config.healing_increment {
            let event = alice.next_behaviour_event().await;
            assert_eq!(
                event,
                Event::ScoreChanged {
                    peer_id: chad_peer_id,
                    reason: ScoreChangeReason::Increase,
                    score: alice_config.healing_increment * (i + 1)
                }
            );

            time::advance(alice_config.driver_time).await;
        }
    }
}
