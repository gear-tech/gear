// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Peer slot management for the network swarm.
//!
//! The behaviour separates peers into inbound and outbound groups based on the
//! direction of the first established connection. Outbound peers are dialed up
//! to a configured minimum, while inbound peers are accepted up to the normal
//! limit plus a small "overflowing" reserve. Overflowing inbound peers are
//! temporary: if they do not show recent useful activity, they are evicted.
//! Fully disconnected peers stay in a short backoff window so the same peer
//! cannot immediately feed dial storms by being retried over and over.

use crate::utils::{ConnectionMap, NoLimits, PeerAddresses};
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    swarm::{
        CloseConnection, ConnectionClosed, ConnectionDenied, ConnectionId, DialFailure, FromSwarm,
        NetworkBehaviour, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
        dial_opts::DialOpts, dummy,
    },
};
use rand::seq::SliceRandom;
use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    convert::Infallible,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    time,
    time::{Instant, Interval},
};

#[derive(Clone, metrics_derive::Metrics)]
#[metrics(scope = "ethexe_network_slots")]
struct Metrics {
    /// Number of inbound peers (including overflowing ones)
    inbound_peers: metrics::Gauge,
    /// Number of inbound overflowing peers
    inbound_overflowing_peers: metrics::Gauge,
    /// Number of outbound peers
    outbound_peers: metrics::Gauge,
}

/// Slot configuration for [`Behaviour`].
///
/// The limits are tracked per peer, not per connection. Once a peer is first
/// observed as inbound or outbound, later connections keep that direction.
/// The backoff period controls how long a fully disconnected peer stays in
/// `JustDisconnected`, preventing that peer from immediately entering dial
/// storms through repeated redials and reconnect attempts.
pub struct Config {
    inbound_max_peers: u32,
    inbound_overflowing_peers: u32,
    inbound_overflowing_peer_action_timeout: Duration,
    outbound_min_peers: u32,
    outbound_max_peers: u32,
    backoff_period: Duration,
    driver_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inbound_max_peers: 45,
            inbound_overflowing_peers: 5,
            inbound_overflowing_peer_action_timeout: Duration::from_secs(20),
            outbound_min_peers: 25,
            outbound_max_peers: 50,
            backoff_period: Duration::from_secs(5),
            driver_interval: Duration::from_secs(1),
        }
    }
}

impl Config {
    fn incoming_peers_total(&self) -> u32 {
        self.inbound_max_peers + self.inbound_overflowing_peers
    }
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
enum SlotConnectionError {
    #[display("limit of {limit} {direction} peers exceeded")]
    LimitExceeded {
        limit: u32,
        direction: PeerDirection,
    },
    #[display("backoff period is active")]
    ActiveBackoffPeriod,
}

#[cfg(test)]
impl SlotConnectionError {
    fn unwrap_limit_exceeded(self) -> (u32, PeerDirection) {
        match self {
            SlotConnectionError::LimitExceeded { limit, direction } => (limit, direction),
            err => panic!("unexpected variant: {err}"),
        }
    }
}

impl std::error::Error for SlotConnectionError {}

impl From<SlotConnectionError> for ConnectionDenied {
    fn from(value: SlotConnectionError) -> Self {
        ConnectionDenied::new(value)
    }
}

#[derive(Debug, Eq, PartialEq)]
enum PeerState {
    Connected {
        connections: HashSet<ConnectionId>,
        direction: PeerDirection,
    },
    JustDisconnected(Instant),
}

impl PeerState {
    fn as_inbound_direction_mut(&mut self) -> Option<&mut InboundPeerDirection> {
        match self {
            PeerState::Connected {
                direction: PeerDirection::Inbound(inbound),
                ..
            } => Some(inbound),
            _ => None,
        }
    }
}

#[cfg(test)]
impl PeerState {
    fn as_direction(&self) -> Option<&PeerDirection> {
        match self {
            PeerState::Connected { direction, .. } => Some(direction),
            PeerState::JustDisconnected(_) => None,
        }
    }

    fn as_inbound_direction(&self) -> Option<&InboundPeerDirection> {
        match self {
            PeerState::Connected {
                direction: PeerDirection::Inbound(inbound),
                ..
            } => Some(inbound),
            _ => None,
        }
    }

    fn unwrap_connected_ref(&self) -> (&HashSet<ConnectionId>, &PeerDirection) {
        match self {
            PeerState::Connected {
                connections,
                direction,
            } => (connections, direction),
            state => unreachable!("unexpected variant: {state:?}"),
        }
    }
}

#[derive(Debug, Eq, PartialEq, derive_more::IsVariant)]
enum InboundPeerDirection {
    Normal,
    Overflowing { latest_action: Instant },
}

#[derive(Debug, Eq, PartialEq, derive_more::Display, derive_more::IsVariant)]
enum PeerDirection {
    #[display("inbound")]
    Inbound(InboundPeerDirection),
    #[display("outbound")]
    Outbound,
}

impl PeerDirection {
    fn is_evictable_overflowing_inbound(&self, timeout: Duration) -> bool {
        match self {
            Self::Inbound(InboundPeerDirection::Overflowing { latest_action }) => {
                latest_action.elapsed() > timeout
            }
            Self::Inbound(InboundPeerDirection::Normal) => false,
            Self::Outbound => false,
        }
    }

    fn increment_metrics(&self, metrics: &Metrics) {
        match self {
            PeerDirection::Inbound(inbound) => {
                metrics.inbound_peers.increment(1);
                if inbound.is_overflowing() {
                    metrics.inbound_overflowing_peers.increment(1);
                }
            }
            PeerDirection::Outbound => {
                metrics.outbound_peers.increment(1);
            }
        }
    }

    fn decrement_metrics(&self, metrics: &Metrics) {
        match self {
            PeerDirection::Inbound(inbound) => {
                metrics.inbound_peers.decrement(1);
                if inbound.is_overflowing() {
                    metrics.inbound_overflowing_peers.decrement(1);
                }
            }
            PeerDirection::Outbound => {
                metrics.outbound_peers.decrement(1);
            }
        }
    }
}

/// Per-peer slot manager used inside the main network behaviour.
///
/// Responsibilities:
/// - enforce inbound and outbound peer limits
/// - keep a short post-disconnect backoff window so the same peer cannot
///   immediately participate in dial storms again
/// - evict idle overflowing inbound peers
/// - schedule outbound dials until the minimum outbound peer count is reached
pub struct Behaviour {
    config: Config,
    pending_outbound_peers: ConnectionMap<NoLimits>,
    peers: HashMap<PeerId, PeerState>,
    pending_events: VecDeque<ToSwarm<Infallible, Infallible>>,
    addresses: PeerAddresses,
    driver: Interval,
    metrics: Metrics,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        Self {
            driver: time::interval(config.driver_interval),
            config,
            pending_outbound_peers: ConnectionMap::without_limits(),
            peers: HashMap::new(),
            pending_events: VecDeque::new(),
            addresses: Default::default(),
            metrics: Metrics::default(),
        }
    }

    /// Marks recent useful activity for a connected peer.
    ///
    /// Only overflowing inbound peers use this signal. Their eviction timeout is
    /// measured from the latest reported action.
    pub(crate) fn report_peer_action(&mut self, peer: &PeerId) {
        let entry = self
            .peers
            .get_mut(peer)
            .expect("we track all connected peers");
        if let Some(InboundPeerDirection::Overflowing { latest_action }) =
            entry.as_inbound_direction_mut()
        {
            *latest_action = Instant::now();
        }
    }

    fn connected_peers(&self) -> impl Iterator<Item = (&PeerId, &PeerDirection)> {
        self.peers.iter().filter_map(|(peer, entry)| match entry {
            PeerState::Connected { direction, .. } => Some((peer, direction)),
            PeerState::JustDisconnected(_) => None,
        })
    }

    fn inbound_peers(&self) -> impl Iterator<Item = (&PeerId, &PeerDirection)> {
        self.connected_peers()
            .filter(|(_peer, direction)| direction.is_inbound())
    }

    fn outbound_peers(&self) -> impl Iterator<Item = (&PeerId, &PeerDirection)> {
        self.connected_peers()
            .filter(|(_peer, direction)| direction.is_outbound())
    }

    fn add_pending_outbound_connection(
        &mut self,
        peer: PeerId,
        connection: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        // no need to track already connected peer, but peers in backoff must still be denied.
        if let Some(entry) = self.peers.get(&peer) {
            if let PeerState::JustDisconnected(_) = entry {
                return Err(SlotConnectionError::ActiveBackoffPeriod.into());
            }

            return Ok(());
        }

        if self.outbound_peers().count() >= self.config.outbound_max_peers as usize {
            return Err(SlotConnectionError::LimitExceeded {
                limit: self.config.outbound_max_peers,
                direction: PeerDirection::Outbound,
            }
            .into());
        }

        let Ok(_added) = self.pending_outbound_peers.add_connection(peer, connection);

        Ok(())
    }

    fn remove_pending_outbound_connection(&mut self, peer: PeerId, connection: ConnectionId) {
        self.pending_outbound_peers
            .remove_connection(peer, connection);
    }

    fn add_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
        mut direction: PeerDirection,
    ) -> Result<(), ConnectionDenied> {
        // existing peers keep the direction of their first connection
        if let Some(entry) = self.peers.get_mut(&peer) {
            return match entry {
                PeerState::Connected { connections, .. } => {
                    connections.insert(connection_id);
                    Ok(())
                }
                PeerState::JustDisconnected(_) => {
                    Err(SlotConnectionError::ActiveBackoffPeriod.into())
                }
            };
        }

        let (limit, peers) = match direction {
            PeerDirection::Inbound(_) => {
                (self.config.inbound_max_peers, self.inbound_peers().count())
            }
            PeerDirection::Outbound => (
                self.config.outbound_max_peers,
                self.outbound_peers().count(),
            ),
        };

        // if we exceed the inbound connection limit, then check we have a free overflowing slot
        #[rustfmt::skip]
        let is_overflowing_inbound_connection =
            direction.is_inbound()
            && peers >= limit as usize
            && self.config.incoming_peers_total().saturating_sub(peers as u32) > 0;
        if peers >= limit as usize && !is_overflowing_inbound_connection {
            return Err(SlotConnectionError::LimitExceeded { limit, direction }.into());
        }

        if let PeerDirection::Inbound(direction) = &mut direction
            && is_overflowing_inbound_connection
        {
            *direction = InboundPeerDirection::Overflowing {
                latest_action: Instant::now(),
            };
        }

        direction.increment_metrics(&self.metrics);

        let old_peer = self.peers.insert(
            peer,
            PeerState::Connected {
                connections: [connection_id].into(),
                direction,
            },
        );
        debug_assert_eq!(old_peer, None);

        Ok(())
    }

    fn add_inbound_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.add_connection(
            peer,
            connection_id,
            PeerDirection::Inbound(InboundPeerDirection::Normal),
        )
    }

    fn add_outbound_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.add_connection(peer, connection_id, PeerDirection::Outbound)
    }

    fn remove_connection(&mut self, peer: PeerId, connection_id: ConnectionId) -> bool {
        match self.peers.entry(peer) {
            Entry::Occupied(mut entry) => {
                let entry = entry.get_mut();
                match entry {
                    PeerState::Connected {
                        connections,
                        direction,
                    } => {
                        connections.remove(&connection_id);
                        if connections.is_empty() {
                            direction.decrement_metrics(&self.metrics);
                            *entry = PeerState::JustDisconnected(Instant::now());
                        }
                    }
                    PeerState::JustDisconnected(_) => {
                        debug_assert!(false, "unexpected {peer} state: {entry:?}")
                    }
                }

                true
            }
            Entry::Vacant(_) => false,
        }
    }

    fn update_on_periods(&mut self) {
        self.peers.retain(|_peer, entry| match entry {
            PeerState::Connected { .. } => true,
            PeerState::JustDisconnected(at) => at.elapsed() <= self.config.backoff_period,
        });
    }

    fn evict_inbound_overflowing_peers(&mut self) {
        let peers = self
            .inbound_peers()
            .filter(|(_peer, direction)| {
                direction.is_evictable_overflowing_inbound(
                    self.config.inbound_overflowing_peer_action_timeout,
                )
            })
            .map(|(&peer, _direction)| peer)
            .collect::<Vec<_>>();

        for peer_id in peers {
            self.pending_events.push_back(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            })
        }
    }

    fn dial_peers(&mut self) {
        // pending outbound dials count towards the minimum to avoid repeated
        // dial scheduling for the same deficit
        let active_outbound_peers = self.outbound_peers().count();
        let pending_outbound_peers = self.pending_outbound_peers.peers().len();
        let needed_outbound_peers = (self.config.outbound_min_peers as usize)
            .saturating_sub(active_outbound_peers)
            .saturating_sub(pending_outbound_peers);
        if needed_outbound_peers == 0 {
            return;
        }

        let mut peers: Vec<_> = self
            .addresses
            .iter()
            .filter(|(peer, _)| {
                !self.pending_outbound_peers.contains_peer(peer) && !self.peers.contains_key(peer)
            })
            .collect();
        peers.shuffle(&mut rand::thread_rng());
        let peers = peers.into_iter().take(needed_outbound_peers);

        for (&peer, addresses) in peers {
            let addresses = addresses.into_iter().cloned().collect();
            let opts = DialOpts::peer_id(peer)
                .addresses(addresses)
                .extend_addresses_through_behaviour()
                .build();
            self.pending_events.push_back(ToSwarm::Dial { opts });
        }
    }

    fn on_driver_tick(&mut self) {
        self.update_on_periods();
        self.evict_inbound_overflowing_peers();
        self.dial_peers();
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Infallible;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.add_inbound_connection(peer, connection_id)?;

        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        // we cannot track unknown peer, so actual limiting is enforced when peer identity is known
        let Some(peer) = maybe_peer else {
            return Ok(vec![]);
        };

        self.add_pending_outbound_connection(peer, connection_id)?;

        Ok(vec![])
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.remove_pending_outbound_connection(peer, connection_id);
        self.add_outbound_connection(peer, connection_id)?;

        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.addresses.on_swarm_event(&event);

        match event {
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                endpoint: _,
                cause: _,
                remaining_established: _,
            }) => {
                self.remove_connection(peer_id, connection_id);
            }
            FromSwarm::DialFailure(DialFailure {
                peer_id: Some(peer_id),
                error: _,
                connection_id,
            }) => {
                self.remove_pending_outbound_connection(peer_id, connection_id);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        _peer_id: PeerId,
        _connection_id: ConnectionId,
        _event: THandlerOutEvent<Self>,
    ) {
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(to_swarm) = self.pending_events.pop_front() {
            return Poll::Ready(to_swarm);
        }

        if let Poll::Ready(_instant) = self.driver.poll_tick(cx) {
            self.on_driver_tick();
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use assert_matches::assert_matches;
    use libp2p::{
        Swarm,
        swarm::{DialError, ListenError, SwarmEvent},
    };
    use libp2p_swarm_test::SwarmExt;

    async fn new_swarm_with_config(config: Config) -> Swarm<Behaviour> {
        let behaviour = Behaviour::new(config);
        let mut swarm = Swarm::new_ephemeral_tokio(|_keypair| behaviour);
        swarm.listen().with_memory_addr_external().await;
        swarm
    }

    async fn new_swarm() -> Swarm<Behaviour> {
        new_swarm_with_config(Config::default()).await
    }

    fn random_multiaddr() -> Multiaddr {
        let port: u64 = rand::random();
        format!("/memory/{port}").parse().unwrap()
    }

    fn drain_dialled_peers(behaviour: &mut Behaviour) -> Vec<PeerId> {
        behaviour
            .pending_events
            .drain(..)
            .map(|event| match event {
                ToSwarm::Dial { opts } => opts.get_peer_id().expect("peer id is set"),
                event => panic!("unexpected event: {event:?}"),
            })
            .collect()
    }

    fn drain_evicted_peers(behaviour: &mut Behaviour) -> Vec<PeerId> {
        behaviour
            .pending_events
            .drain(..)
            .map(|event| match event {
                ToSwarm::CloseConnection {
                    peer_id,
                    connection: CloseConnection::All,
                } => peer_id,
                event => panic!("unexpected event: {event:?}"),
            })
            .collect()
    }

    #[tokio::test]
    async fn inbound_peers_limit() {
        init_logger();

        let mut alice = new_swarm().await;
        let alice_peer_id = *alice.local_peer_id();
        let alice_addrs = alice.external_addresses().cloned().collect();

        for _ in 0..Config::default().incoming_peers_total() {
            let mut peer = new_swarm().await;
            peer.connect(&mut alice).await;
            tokio::spawn(peer.loop_on_next());
        }

        let mut bob = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();
        bob.dial(
            DialOpts::peer_id(alice_peer_id)
                .addresses(alice_addrs)
                .build(),
        )
        .unwrap();
        tokio::spawn(bob.loop_on_next());

        let event = alice.next_swarm_event().await;
        assert_matches!(event, SwarmEvent::IncomingConnection { .. });

        let event = alice.next_swarm_event().await;
        if let SwarmEvent::IncomingConnectionError {
            error: ListenError::Denied { cause },
            peer_id: Some(peer_id),
            ..
        } = event
        {
            assert_eq!(peer_id, bob_peer_id);
            let (limit, direction) = cause
                .downcast::<SlotConnectionError>()
                .unwrap()
                .unwrap_limit_exceeded();
            assert_eq!(limit, Config::default().inbound_max_peers);
            assert_eq!(
                direction,
                PeerDirection::Inbound(InboundPeerDirection::Normal)
            );
        } else {
            unreachable!("unexpected event: {event:?}");
        }
    }

    #[tokio::test]
    async fn outbound_peers_limit() {
        init_logger();

        let mut alice = new_swarm().await;

        for _ in 0..Config::default().outbound_max_peers {
            let mut peer = new_swarm().await;
            alice.connect(&mut peer).await;
            tokio::spawn(peer.loop_on_next());
        }

        let bob = new_swarm().await;
        let bob_peer_id = *bob.local_peer_id();
        let bob_addrs = bob.external_addresses().cloned().collect();
        tokio::spawn(bob.loop_on_next());

        let err = alice
            .dial(DialOpts::peer_id(bob_peer_id).addresses(bob_addrs).build())
            .unwrap_err();
        let DialError::Denied { cause } = err else {
            unreachable!("unexpected error: {err:?}");
        };
        let (limit, direction) = cause
            .downcast::<SlotConnectionError>()
            .unwrap()
            .unwrap_limit_exceeded();
        assert_eq!(limit, Config::default().outbound_max_peers);
        assert_eq!(direction, PeerDirection::Outbound);
    }

    #[tokio::test]
    async fn add_inbound_connection_uses_overflowing_slots_after_normal_limit() {
        let config = Config {
            inbound_max_peers: 1,
            inbound_overflowing_peers: 1,
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        let normal_peer_id = PeerId::random();
        behaviour
            .add_inbound_connection(normal_peer_id, ConnectionId::new_unchecked(1))
            .unwrap();

        let overflowing_peer_id = PeerId::random();
        behaviour
            .add_inbound_connection(overflowing_peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        assert_eq!(
            behaviour
                .peers
                .get(&normal_peer_id)
                .and_then(|entry| entry.as_inbound_direction()),
            Some(&InboundPeerDirection::Normal)
        );
        assert_matches!(
            behaviour
                .peers
                .get(&overflowing_peer_id)
                .and_then(|entry| entry.as_inbound_direction()),
            Some(InboundPeerDirection::Overflowing { .. })
        );
    }

    #[tokio::test]
    async fn add_inbound_connection_rejects_when_all_inbound_slots_are_used() {
        let config = Config {
            inbound_max_peers: 1,
            inbound_overflowing_peers: 1,
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        behaviour
            .add_inbound_connection(PeerId::random(), ConnectionId::new_unchecked(1))
            .unwrap();
        behaviour
            .add_inbound_connection(PeerId::random(), ConnectionId::new_unchecked(2))
            .unwrap();

        let err = behaviour
            .add_inbound_connection(PeerId::random(), ConnectionId::new_unchecked(3))
            .unwrap_err();
        let (limit, direction) = err
            .downcast::<SlotConnectionError>()
            .unwrap()
            .unwrap_limit_exceeded();
        assert_eq!(limit, behaviour.config.inbound_max_peers);
        assert_eq!(
            direction,
            PeerDirection::Inbound(InboundPeerDirection::Normal)
        );
    }

    #[tokio::test]
    async fn add_outbound_connection_allows_multiple_connections_for_known_peer_at_limit() {
        let config = Config {
            outbound_max_peers: 1,
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        let known_peer_id = PeerId::random();
        behaviour
            .add_outbound_connection(known_peer_id, ConnectionId::new_unchecked(1))
            .unwrap();
        behaviour
            .add_outbound_connection(known_peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        let (connections, direction) = behaviour
            .peers
            .get(&known_peer_id)
            .unwrap()
            .unwrap_connected_ref();
        assert_eq!(*direction, PeerDirection::Outbound);
        assert_eq!(
            *connections,
            [
                ConnectionId::new_unchecked(1),
                ConnectionId::new_unchecked(2)
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[tokio::test]
    async fn add_outbound_connection_rejects_peer_in_backoff_period() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        let first_connection_id = ConnectionId::new_unchecked(1);
        behaviour
            .add_outbound_connection(peer_id, first_connection_id)
            .unwrap();
        behaviour.remove_connection(peer_id, first_connection_id);

        let err = behaviour
            .add_outbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap_err()
            .downcast::<SlotConnectionError>()
            .unwrap();
        assert_eq!(err, SlotConnectionError::ActiveBackoffPeriod);
    }

    #[tokio::test]
    async fn add_inbound_connection_rejects_peer_in_backoff_period() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        let first_connection_id = ConnectionId::new_unchecked(1);
        behaviour
            .add_inbound_connection(peer_id, first_connection_id)
            .unwrap();
        behaviour.remove_connection(peer_id, first_connection_id);

        let err = behaviour
            .add_inbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap_err()
            .downcast::<SlotConnectionError>()
            .unwrap();
        assert_eq!(err, SlotConnectionError::ActiveBackoffPeriod);
    }

    #[tokio::test]
    async fn add_pending_outbound_connection_does_not_track_known_peer() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        behaviour
            .add_inbound_connection(peer_id, ConnectionId::new_unchecked(1))
            .unwrap();

        behaviour
            .add_pending_outbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        assert!(!behaviour.pending_outbound_peers.contains_peer(&peer_id));
    }

    #[tokio::test]
    async fn add_pending_outbound_connection_rejects_known_peer_in_backoff_period() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        let first_connection_id = ConnectionId::new_unchecked(1);
        behaviour
            .add_outbound_connection(peer_id, first_connection_id)
            .unwrap();
        behaviour.remove_connection(peer_id, first_connection_id);

        let err = behaviour
            .add_pending_outbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap_err()
            .downcast::<SlotConnectionError>()
            .unwrap();
        assert_eq!(err, SlotConnectionError::ActiveBackoffPeriod);
        assert!(!behaviour.pending_outbound_peers.contains_peer(&peer_id));
    }

    #[tokio::test]
    async fn add_pending_outbound_connection_ignores_backoff_peers_for_limit() {
        let mut behaviour = Behaviour::new(Config::default());

        behaviour.peers.insert(
            PeerId::random(),
            PeerState::JustDisconnected(Instant::now()),
        );

        let peer_id = PeerId::random();
        let connection_id = ConnectionId::new_unchecked(1);
        behaviour
            .add_pending_outbound_connection(peer_id, connection_id)
            .unwrap();

        assert!(behaviour.pending_outbound_peers.contains_peer(&peer_id));
    }

    #[tokio::test]
    async fn on_swarm_event_dial_failure_removes_pending_outbound_peer() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        let connection_id = ConnectionId::new_unchecked(1);
        behaviour
            .add_pending_outbound_connection(peer_id, connection_id)
            .unwrap();

        let dial_error = DialError::Aborted;
        behaviour.on_swarm_event(FromSwarm::DialFailure(DialFailure {
            peer_id: Some(peer_id),
            error: &dial_error,
            connection_id,
        }));

        assert!(!behaviour.pending_outbound_peers.contains_peer(&peer_id));
    }

    #[tokio::test]
    async fn add_outbound_connection_keeps_initial_inbound_direction() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        behaviour
            .add_inbound_connection(peer_id, ConnectionId::new_unchecked(1))
            .unwrap();
        behaviour
            .add_outbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        let (connections, direction) = behaviour
            .peers
            .get(&peer_id)
            .unwrap()
            .unwrap_connected_ref();
        assert_eq!(
            *direction,
            PeerDirection::Inbound(InboundPeerDirection::Normal)
        );
        assert_eq!(
            *connections,
            [
                ConnectionId::new_unchecked(1),
                ConnectionId::new_unchecked(2)
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[tokio::test]
    async fn add_inbound_connection_keeps_initial_outbound_direction() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        behaviour
            .add_outbound_connection(peer_id, ConnectionId::new_unchecked(1))
            .unwrap();
        behaviour
            .add_inbound_connection(peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        let (connections, direction) = behaviour
            .peers
            .get(&peer_id)
            .unwrap()
            .unwrap_connected_ref();
        assert_eq!(*direction, PeerDirection::Outbound);
        assert_eq!(
            *connections,
            [
                ConnectionId::new_unchecked(1),
                ConnectionId::new_unchecked(2)
            ]
            .into_iter()
            .collect::<HashSet<_>>()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn report_peer_action_updates_latest_action_for_overflowing_inbound_peer() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        let initial_action_at = Instant::now();
        behaviour.peers.insert(
            peer_id,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(1)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Overflowing {
                    latest_action: initial_action_at,
                }),
            },
        );

        time::advance(Duration::from_millis(1)).await;
        behaviour.report_peer_action(&peer_id);

        let (_, direction) = behaviour
            .peers
            .get(&peer_id)
            .unwrap()
            .unwrap_connected_ref();
        let updated_action_at = match direction {
            PeerDirection::Inbound(InboundPeerDirection::Overflowing { latest_action }) => {
                *latest_action
            }
            direction => panic!("unexpected direction: {direction:?}"),
        };
        assert!(updated_action_at > initial_action_at);
    }

    #[tokio::test]
    async fn report_peer_action_is_noop_for_non_overflowing_peers() {
        let mut behaviour = Behaviour::new(Config::default());

        let normal_inbound_peer_id = PeerId::random();
        behaviour.peers.insert(
            normal_inbound_peer_id,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(1)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Normal),
            },
        );

        let outbound_peer_id = PeerId::random();
        behaviour.peers.insert(
            outbound_peer_id,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(2)].into(),
                direction: PeerDirection::Outbound,
            },
        );

        behaviour.report_peer_action(&normal_inbound_peer_id);
        behaviour.report_peer_action(&outbound_peer_id);

        assert_eq!(
            behaviour
                .peers
                .get(&normal_inbound_peer_id)
                .and_then(|entry| entry.as_direction()),
            Some(&PeerDirection::Inbound(InboundPeerDirection::Normal))
        );
        assert_eq!(
            behaviour
                .peers
                .get(&outbound_peer_id)
                .and_then(|entry| entry.as_direction()),
            Some(&PeerDirection::Outbound)
        );
    }

    #[tokio::test]
    async fn dial_peers_dials_all_needed_known_peers() {
        init_logger();

        let mut alice = new_swarm().await;

        let mut peers = [PeerId::random(), PeerId::random(), PeerId::random()];
        for peer in peers {
            alice.add_peer_address(peer, random_multiaddr());
        }

        alice.behaviour_mut().dial_peers();

        let mut dialled = drain_dialled_peers(alice.behaviour_mut());
        dialled.sort();
        peers.sort();
        assert_eq!(dialled, peers);
    }

    #[tokio::test]
    async fn dial_peers_skips_connected_and_pending_peers() {
        let mut alice = new_swarm().await;

        let mut outbound_peer = new_swarm().await;
        alice.connect(&mut outbound_peer).await;
        tokio::spawn(outbound_peer.loop_on_next());

        let mut inbound_peer = new_swarm().await;
        inbound_peer.connect(&mut alice).await;
        tokio::spawn(inbound_peer.loop_on_next());

        // pending outbound peer
        alice
            .dial(
                DialOpts::peer_id(PeerId::random())
                    .addresses(vec![random_multiaddr()])
                    .build(),
            )
            .unwrap();

        let eligible_peer_id = PeerId::random();
        alice.add_peer_address(eligible_peer_id, random_multiaddr());

        alice.behaviour_mut().dial_peers();

        let dialled = drain_dialled_peers(alice.behaviour_mut());
        assert_eq!(dialled, [eligible_peer_id]);
    }

    #[tokio::test]
    async fn dial_peers_is_noop_when_minimum_is_already_satisfied() {
        let config = Config {
            outbound_min_peers: 2,
            ..Default::default()
        };
        let mut alice = new_swarm_with_config(config).await;

        let mut outbound_peer = new_swarm().await;
        alice.connect(&mut outbound_peer).await;

        // pending outbound peer
        alice
            .dial(
                DialOpts::peer_id(PeerId::random())
                    .addresses(vec![random_multiaddr()])
                    .build(),
            )
            .unwrap();

        alice.add_peer_address(PeerId::random(), random_multiaddr());

        alice.behaviour_mut().dial_peers();

        assert!(alice.behaviour_mut().pending_events.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn dial_peers_ignores_backoff_peers_when_counting_outbound_minimum() {
        let config = Config {
            outbound_min_peers: 2,
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        behaviour.peers.insert(
            PeerId::random(),
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(1)].into(),
                direction: PeerDirection::Outbound,
            },
        );
        behaviour.peers.insert(
            PeerId::random(),
            PeerState::JustDisconnected(Instant::now()),
        );

        let replacement_peer = PeerId::random();
        behaviour
            .addresses
            .add(replacement_peer, random_multiaddr());

        behaviour.dial_peers();

        assert_eq!(drain_dialled_peers(&mut behaviour), [replacement_peer]);
    }

    #[tokio::test(start_paused = true)]
    async fn update_on_periods_removes_just_disconnected_only_after_backoff_period() {
        let config = Config {
            backoff_period: Duration::from_secs(5),
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        let disconnected_peer_id = PeerId::random();
        behaviour.peers.insert(
            disconnected_peer_id,
            PeerState::JustDisconnected(Instant::now()),
        );

        let connected_peer_id = PeerId::random();
        behaviour.peers.insert(
            connected_peer_id,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(2)].into(),
                direction: PeerDirection::Outbound,
            },
        );

        // the backoff period is not ended
        behaviour.update_on_periods();
        assert!(behaviour.peers.contains_key(&disconnected_peer_id));
        assert_matches!(
            behaviour.peers.get(&connected_peer_id),
            Some(PeerState::Connected { .. })
        );

        // the backoff period is exactly ended
        time::advance(behaviour.config.backoff_period).await;
        behaviour.update_on_periods();
        assert!(behaviour.peers.contains_key(&disconnected_peer_id));

        // after the backoff period peer must be removed
        time::advance(Duration::from_millis(1)).await;
        behaviour.update_on_periods();
        assert!(!behaviour.peers.contains_key(&disconnected_peer_id));
        assert_matches!(
            behaviour.peers.get(&connected_peer_id),
            Some(PeerState::Connected { .. })
        );
    }

    #[tokio::test(start_paused = true)]
    async fn evict_inbound_overflowing_peers_closes_only_evictable_peers() {
        let mut behaviour = Behaviour::new(Config::default());

        let stale_overflowing = PeerId::random();
        behaviour.peers.insert(
            stale_overflowing,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(1)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Overflowing {
                    latest_action: Instant::now(),
                }),
            },
        );

        let refreshed_overflowing = PeerId::random();
        behaviour.peers.insert(
            refreshed_overflowing,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(2)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Overflowing {
                    latest_action: Instant::now(),
                }),
            },
        );

        let normal_inbound = PeerId::random();
        behaviour.peers.insert(
            normal_inbound,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(3)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Normal),
            },
        );

        let outbound = PeerId::random();
        behaviour.peers.insert(
            outbound,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(4)].into(),
                direction: PeerDirection::Outbound,
            },
        );

        time::advance(behaviour.config.inbound_overflowing_peer_action_timeout).await;
        behaviour.report_peer_action(&refreshed_overflowing);
        time::advance(Duration::from_millis(1)).await;

        behaviour.evict_inbound_overflowing_peers();

        assert_eq!(drain_evicted_peers(&mut behaviour), [stale_overflowing]);
    }

    #[tokio::test(start_paused = true)]
    async fn evict_inbound_overflowing_peers_waits_until_timeout_is_exceeded() {
        let mut behaviour = Behaviour::new(Config::default());

        let peer_id = PeerId::random();
        behaviour.peers.insert(
            peer_id,
            PeerState::Connected {
                connections: [ConnectionId::new_unchecked(1)].into(),
                direction: PeerDirection::Inbound(InboundPeerDirection::Overflowing {
                    latest_action: Instant::now(),
                }),
            },
        );

        time::advance(behaviour.config.inbound_overflowing_peer_action_timeout).await;
        behaviour.evict_inbound_overflowing_peers();
        assert!(behaviour.pending_events.is_empty());

        time::advance(Duration::from_millis(1)).await;
        behaviour.evict_inbound_overflowing_peers();
        assert_eq!(drain_evicted_peers(&mut behaviour), [peer_id]);
    }

    #[tokio::test(start_paused = true)]
    async fn evict_inbound_overflowing_peers_does_not_evict_fresh_overflowing_peer() {
        let config = Config {
            inbound_max_peers: 1,
            inbound_overflowing_peers: 1,
            ..Default::default()
        };
        let mut behaviour = Behaviour::new(config);

        behaviour
            .add_inbound_connection(PeerId::random(), ConnectionId::new_unchecked(1))
            .unwrap();

        let overflowing_peer_id = PeerId::random();
        behaviour
            .add_inbound_connection(overflowing_peer_id, ConnectionId::new_unchecked(2))
            .unwrap();

        behaviour.evict_inbound_overflowing_peers();
        assert!(behaviour.pending_events.is_empty());

        time::advance(behaviour.config.inbound_overflowing_peer_action_timeout).await;
        behaviour.evict_inbound_overflowing_peers();
        assert!(behaviour.pending_events.is_empty());

        time::advance(Duration::from_millis(1)).await;
        behaviour.evict_inbound_overflowing_peers();
        assert_eq!(drain_evicted_peers(&mut behaviour), [overflowing_peer_id]);
    }
}
