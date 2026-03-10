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

use crate::db_sync::{Multiaddr, PeerId};
use futures::{FutureExt, StreamExt, future::BoxFuture, stream::FuturesUnordered};
use libp2p::{
    core::{Endpoint, transport::PortUse},
    swarm::{
        CloseConnection, ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm,
        NetworkBehaviour, PeerAddresses, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
        dial_opts::DialOpts, dummy,
    },
};
use rand::seq::{IteratorRandom, SliceRandom};
use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    convert::Infallible,
    num::NonZeroUsize,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{time, time::Interval};

pub struct Config {
    inbound_max_peers: u32,
    outbound_min_peers: u32,
    outbound_max_peers: u32,
    grace_period: Duration,
    backoff_period: Duration,
    driver_interval: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inbound_max_peers: 50,
            outbound_min_peers: 25,
            outbound_max_peers: 50,
            grace_period: Duration::from_secs(5),
            backoff_period: Duration::from_secs(5),
            driver_interval: Duration::from_secs(1),
        }
    }
}

#[derive(Debug, derive_more::Display)]
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

    fn unwrap_active_backoff_period(self) {
        match self {
            SlotConnectionError::ActiveBackoffPeriod => (),
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
    JustConnected,
    Connected,
    JustDisconnected,
}

#[derive(Debug, Eq, PartialEq, derive_more::Display, derive_more::IsVariant)]
enum PeerDirection {
    #[display("inbound")]
    Inbound,
    #[display("outbound")]
    Outbound,
}

#[derive(Debug, Eq, PartialEq)]
struct PeerEntry {
    connections: HashSet<ConnectionId>,
    direction: PeerDirection,
    state: PeerState,
}

enum PeriodKind {
    GracePeriod,
    BackoffPeriod,
}

type PeriodFuture = BoxFuture<'static, (PeerId, PeriodKind)>;

pub struct Behaviour {
    config: Config,
    peers: HashMap<PeerId, PeerEntry>,
    pending_events: VecDeque<ToSwarm<Infallible, Infallible>>,
    addresses: PeerAddresses,
    driver: Interval,
    /// Track grace and backoff periods
    periods: FuturesUnordered<PeriodFuture>,
    /// How many peers we are dialing currently
    pending_outbound_peers: usize,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        Self {
            driver: time::interval(config.driver_interval),
            config,
            peers: HashMap::new(),
            pending_events: VecDeque::new(),
            addresses: Default::default(),
            periods: FuturesUnordered::new(),
            pending_outbound_peers: 0,
        }
    }

    fn inbound_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers
            .iter()
            .filter(|(peer, entry)| entry.direction == PeerDirection::Inbound)
            .map(|(peer, _)| peer)
    }

    fn outbound_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers
            .iter()
            .filter(|(peer, entry)| entry.direction == PeerDirection::Outbound)
            .map(|(peer, _)| peer)
    }

    fn evict_peer(&mut self) -> bool {
        let peer = self
            .peers
            .iter()
            .filter(|(_peer, entry)| entry.state == PeerState::Connected)
            .choose_stable(&mut rand::thread_rng());

        if let Some((&peer, _entry)) = peer {
            self.pending_events.push_back(ToSwarm::CloseConnection {
                peer_id: peer,
                connection: CloseConnection::All,
            });
            true
        } else {
            false
        }
    }

    fn add_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
        direction: PeerDirection,
    ) -> Result<(), ConnectionDenied> {
        let (limit, peers) = match direction {
            PeerDirection::Inbound => (
                self.config.inbound_max_peers,
                itertools::Either::Left(self.inbound_peers()),
            ),
            PeerDirection::Outbound => (
                self.config.outbound_max_peers,
                itertools::Either::Right(self.outbound_peers()),
            ),
        };

        // check if limit exceeded, but try to evict a peer if the connection is incoming
        if peers.count() >= limit as usize && !(direction.is_inbound() && self.evict_peer()) {
            return Err(SlotConnectionError::LimitExceeded { limit, direction }.into());
        }

        let mut entry = match self.peers.entry(peer) {
            Entry::Occupied(entry) => {
                if let PeerState::JustDisconnected = entry.get().state {
                    return Err(SlotConnectionError::ActiveBackoffPeriod.into());
                }

                entry
            }
            Entry::Vacant(entry) => {
                let grace_period = self.config.grace_period;
                self.periods.push(
                    async move {
                        time::sleep(grace_period).await;
                        (peer, PeriodKind::GracePeriod)
                    }
                    .boxed(),
                );

                entry.insert_entry(PeerEntry {
                    connections: Default::default(),
                    direction,
                    state: PeerState::JustConnected,
                })
            }
        };

        entry.get_mut().connections.insert(connection_id);

        Ok(())
    }

    fn add_inbound_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.add_connection(peer, connection_id, PeerDirection::Inbound)
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
                let peer_entry = entry.get_mut();
                peer_entry.connections.remove(&connection_id);
                if peer_entry.connections.is_empty() {
                    debug_assert_eq!(peer_entry.state, PeerState::Connected);
                    peer_entry.state = PeerState::JustDisconnected;

                    let backoff_period = self.config.backoff_period;
                    self.periods.push(
                        async move {
                            time::sleep(backoff_period).await;
                            (peer, PeriodKind::BackoffPeriod)
                        }
                        .boxed(),
                    )
                }

                true
            }
            Entry::Vacant(_) => false,
        }
    }

    fn on_period_ended(&mut self, peer: PeerId, kind: PeriodKind) {
        match kind {
            PeriodKind::GracePeriod => {
                let entry = self.peers.get_mut(&peer).expect("unknown peer");
                debug_assert_eq!(entry.state, PeerState::JustConnected);
                debug_assert!(!entry.connections.is_empty());
                entry.state = PeerState::Connected;
            }
            PeriodKind::BackoffPeriod => {
                let entry = self.peers.remove(&peer).expect("unknown peer");
                debug_assert_eq!(entry.state, PeerState::JustDisconnected);
                debug_assert_eq!(entry.connections, HashSet::default());
            }
        }
    }

    fn on_driver_tick(&mut self) {
        let outbounds_peers = self.outbound_peers().count();
        let Some(needed_outbound_peers) = (self.config.outbound_min_peers as usize)
            .checked_sub(outbounds_peers)
            .and_then(|peers| peers.checked_sub(self.pending_outbound_peers))
            .and_then(NonZeroUsize::new)
        else {
            return;
        };

        let mut peers: Vec<PeerId> = self.peers.keys().copied().collect();
        peers.shuffle(&mut rand::thread_rng());
        let peers = peers.into_iter().take(needed_outbound_peers.get());

        for peer in peers {
            let addresses: Vec<Multiaddr> = self.addresses.get(&peer).collect();
            let opts = DialOpts::peer_id(peer)
                .addresses(addresses)
                .extend_addresses_through_behaviour()
                .build();
            self.pending_events.push_back(ToSwarm::Dial { opts });
        }
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
        _connection_id: ConnectionId,
        _maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.pending_outbound_peers += 1;

        if self.outbound_peers().count() >= self.config.outbound_max_peers as usize {
            return Err(SlotConnectionError::LimitExceeded {
                limit: self.config.outbound_max_peers,
                direction: PeerDirection::Outbound,
            }
            .into());
        }

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
        self.pending_outbound_peers -= 1;
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
            FromSwarm::DialFailure(_) => {
                self.pending_outbound_peers -= 1;
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
        if let Some(swarm) = self.pending_events.pop_front() {
            return Poll::Ready(swarm);
        }

        if let Poll::Ready(Some((peer, kind))) = self.periods.poll_next_unpin(cx) {
            self.on_period_ended(peer, kind)
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

    #[tokio::test]
    async fn inbound_peers_limit() {
        init_logger();

        let mut alice = new_swarm().await;
        let alice_peer_id = *alice.local_peer_id();
        let alice_addrs = alice.external_addresses().cloned().collect();

        for _ in 0..Config::default().inbound_max_peers {
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
            assert_eq!(direction, PeerDirection::Inbound);
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
}
