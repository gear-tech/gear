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

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_network_peer_score")]
struct Metrics {
    /// Number of blocked peers
    blocked_peers: metrics::Gauge,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScoreDecreaseReason {
    ExcessiveData,
    InvalidData,
}

impl ScoreDecreaseReason {
    fn to_i8(self, config: &Config) -> i8 {
        match self {
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
    /// Peer has been unblocked because of decay
    PeerUnblocked {
        /// Peer we blocked
        peer_id: PeerId,
    },
}

/// Behaviour config.
#[derive(Debug, Clone)]
pub(crate) struct Config {
    excessive_data: i8,
    invalid_data: i8,
    decay: i8,
    blocked_threshold: i8,
    driver_time: Duration,
    forget_time: Duration,
}

impl Config {
    const fn new() -> Self {
        Self {
            excessive_data: i8::MIN / 5,
            invalid_data: i8::MIN / 3,
            decay: i8::MAX / 17,
            blocked_threshold: i8::MIN / 3,
            driver_time: Duration::from_secs(1),
            forget_time: Duration::from_hours(1),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

struct ScoreEntry {
    score: i8,
    updated_at: Instant,
}

impl Default for ScoreEntry {
    fn default() -> Self {
        Self {
            score: 0,
            updated_at: Instant::now(),
        }
    }
}

impl ScoreEntry {
    fn is_expired(&self, forget_time: Duration) -> bool {
        self.score == 0 && self.updated_at + forget_time <= Instant::now()
    }

    fn is_blocked(&self, blocked_threshold: i8) -> bool {
        self.score <= blocked_threshold
    }

    fn add_score(&mut self, score: i8) {
        self.updated_at = Instant::now();
        self.score = self.score.saturating_add(score);
    }

    // NOTE: we don't change the `updated_at` field because we assume decay happens on its own
    fn decay_score(&mut self, decay: i8) {
        // must be always positive to change peer score toward to 0
        debug_assert!(decay.is_positive());

        // we always decay to 0, so in case of overflow/underflow, we clamp the score to 0
        self.score = if self.score.is_positive() {
            self.score.saturating_add(-decay).clamp(0, i8::MAX)
        } else {
            self.score.saturating_add(decay).clamp(i8::MIN, 0)
        };
    }
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
    metrics: Metrics,
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
            metrics: Metrics::default(),
        }
    }

    pub(crate) fn handle(&self) -> Handle {
        self.handle.clone()
    }

    #[cfg(test)]
    fn get_score(&self, peer_id: PeerId) -> Option<i8> {
        self.peers.get(&peer_id).map(|entry| entry.score)
    }

    fn on_driver_tick(&mut self) {
        self.peers.retain(|&peer_id, entry| {
            let was_blocked = entry.is_blocked(self.config.blocked_threshold);
            entry.decay_score(self.config.decay);
            let now_blocked = entry.is_blocked(self.config.blocked_threshold);

            if was_blocked && !now_blocked {
                self.block_list.unblock_peer(peer_id);
                self.pending_events
                    .push_back(Event::PeerUnblocked { peer_id });
            }

            // remove the peer score entry if it is not updated for a long time
            if entry.is_expired(self.config.forget_time) {
                // should be unblocked during decay
                debug_assert!(!self.block_list.blocked_peers().contains(&peer_id));
                return false;
            }

            true
        });

        self.metrics
            .blocked_peers
            .set(self.block_list.blocked_peers().len() as f64);
    }

    fn on_score_decrease(&mut self, peer_id: PeerId, reason: ScoreDecreaseReason) -> Option<Event> {
        let entry = self.peers.entry(peer_id).or_default();

        let was_blocked = entry.is_blocked(self.config.blocked_threshold);
        entry.add_score(reason.to_i8(&self.config));
        let now_blocked = entry.is_blocked(self.config.blocked_threshold);

        if !was_blocked && now_blocked {
            self.block_list.block_peer(peer_id);
            return Some(Event::PeerBlocked {
                peer_id,
                last_reason: reason,
            });
        }

        None
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

        if let Poll::Ready(Some((peer_id, reason))) = self.rx.poll_recv(cx)
            && let Some(event) = self.on_score_decrease(peer_id, reason)
        {
            return Poll::Ready(ToSwarm::GenerateEvent(event));
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use futures::future;
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

    #[tokio::test]
    async fn smoke() {
        const EXCESSIVE_DATA: i8 = Config::new().blocked_threshold / 3 - 1;

        init_logger();

        let alice_config = Config {
            excessive_data: EXCESSIVE_DATA,
            ..Default::default()
        };
        let mut alice = new_swarm_with_config(alice_config.clone()).await;
        let mut chad = new_swarm().await;
        let chad_peer_id = *chad.local_peer_id();
        alice.connect(&mut chad).await;
        tokio::spawn(chad.loop_on_next());

        let handle = alice.behaviour_mut().handle();
        handle.excessive_data(chad_peer_id);

        let event = future::poll_immediate(alice.next_behaviour_event()).await;
        assert_eq!(event, None);
        assert_eq!(
            alice.behaviour().get_score(chad_peer_id),
            Some(EXCESSIVE_DATA)
        );

        handle.excessive_data(chad_peer_id);

        let event = future::poll_immediate(alice.next_behaviour_event()).await;
        assert_eq!(event, None);
        assert_eq!(
            alice.behaviour().get_score(chad_peer_id),
            Some(2 * EXCESSIVE_DATA)
        );

        handle.excessive_data(chad_peer_id);

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::PeerBlocked {
                peer_id: chad_peer_id,
                last_reason: ScoreDecreaseReason::ExcessiveData
            }
        );
        assert_eq!(
            alice.behaviour().get_score(chad_peer_id),
            Some(3 * EXCESSIVE_DATA)
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

        time::sleep(alice_config.driver_time).await;

        let event = alice.next_behaviour_event().await;
        assert_eq!(
            event,
            Event::PeerUnblocked {
                peer_id: chad_peer_id,
            }
        );
        assert_eq!(
            alice.behaviour().get_score(chad_peer_id),
            Some(EXCESSIVE_DATA * 3 + alice_config.decay)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn peer_forgot() {
        init_logger();

        let mut alice = new_swarm().await;
        let alice = alice.behaviour_mut();

        let peer_id = PeerId::random();

        let event = alice.on_score_decrease(peer_id, ScoreDecreaseReason::InvalidData);
        assert!(alice.block_list.blocked_peers().contains(&peer_id));
        assert_eq!(
            event,
            Some(Event::PeerBlocked {
                peer_id,
                last_reason: ScoreDecreaseReason::InvalidData
            })
        );
        assert!(alice.peers.contains_key(&peer_id));

        time::advance(alice.config.forget_time).await;

        // wait for decay
        while alice.get_score(peer_id).is_some_and(|score| score != 0) {
            alice.on_driver_tick();
        }

        assert!(!alice.block_list.blocked_peers().contains(&peer_id));
        assert_eq!(
            alice.pending_events.pop_front(),
            Some(Event::PeerUnblocked { peer_id })
        );
        assert!(!alice.peers.contains_key(&peer_id));
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn decay_math() {
        let mut entry = ScoreEntry::default();

        entry.score = i8::MIN;
        entry.decay_score(i8::MAX);
        assert_eq!(entry.score, -1);
        entry.decay_score(i8::MAX);
        assert_eq!(entry.score, 0);
        entry.decay_score(i8::MAX);
        assert_eq!(entry.score, 0);

        entry.score = i8::MAX;
        entry.decay_score(1);
        assert_eq!(entry.score, i8::MAX - 1);
        entry.decay_score(i8::MAX);
        assert_eq!(entry.score, 0);
        entry.decay_score(i8::MAX);
        assert_eq!(entry.score, 0);
    }
}
