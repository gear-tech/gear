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

use assert_matches::debug_assert_matches;
use itertools::Either;
use libp2p::{
    Multiaddr, PeerId,
    core::{Endpoint, transport::PortUse},
    ping,
    swarm::{
        CloseConnection, ConnectionClosed, ConnectionDenied, ConnectionHandler,
        ConnectionHandlerSelect, ConnectionId, DialFailure, FromSwarm, NetworkBehaviour,
        PeerAddresses, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm, dial_opts::DialOpts,
        dummy,
    },
};
use rand::seq::SliceRandom;
use std::{
    cmp,
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    convert::Infallible,
    num::NonZeroUsize,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    time,
    time::{Instant, Interval},
};

pub struct Config {
    inbound_max_peers: u32,
    inbound_overflowing_peers: u32,
    inbound_overflowing_peer_action_timeout: Duration,
    outbound_min_peers: u32,
    outbound_max_peers: u32,
    grace_period: Duration,
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
    JustDisconnected(Instant),
}

#[derive(Debug, Eq, PartialEq)]
enum InboundPeerDirection {
    Normal,
    Overflowing { latest_action: Option<Instant> },
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
            Self::Inbound(InboundPeerDirection::Overflowing {
                latest_action: Some(latest_action),
            }) => latest_action.elapsed() > timeout,
            Self::Inbound(InboundPeerDirection::Overflowing {
                latest_action: None,
            }) => true,
            Self::Inbound(InboundPeerDirection::Normal) => false,
            Self::Outbound => false,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct PeerEntry {
    connections: HashSet<ConnectionId>,
    direction: PeerDirection,
    state: PeerState,
    lowest_ping: Duration,
    connected_at: Instant,
}

pub struct Behaviour {
    /// Track the lowest peer ping
    ping: ping::Behaviour,
    config: Config,
    peers: HashMap<PeerId, PeerEntry>,
    pending_events: VecDeque<ToSwarm<Infallible, Infallible>>,
    addresses: PeerAddresses,
    driver: Interval,
    /// How many peers we are dialing currently
    pending_outbound_peers: HashSet<PeerId>,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        Self {
            ping: ping::Behaviour::default(),
            driver: time::interval(config.driver_interval),
            config,
            peers: HashMap::new(),
            pending_events: VecDeque::new(),
            addresses: Default::default(),
            pending_outbound_peers: Default::default(),
        }
    }

    pub(crate) fn peer_action(&mut self, peer: &PeerId) {
        let entry = self
            .peers
            .get_mut(peer)
            .expect("we track all connected peers");
        if let PeerDirection::Inbound(InboundPeerDirection::Overflowing { latest_action }) =
            &mut entry.direction
        {
            *latest_action = Some(Instant::now());
        }
    }

    fn inbound_peers(&self) -> impl Iterator<Item = (&PeerId, &PeerEntry)> {
        self.peers
            .iter()
            .filter(|(_peer, entry)| entry.direction.is_inbound())
    }

    fn outbound_peers(&self) -> impl Iterator<Item = &PeerId> {
        self.peers
            .iter()
            .filter(|(_peer, entry)| entry.direction == PeerDirection::Outbound)
            .map(|(peer, _)| peer)
    }

    fn add_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
        direction: PeerDirection,
    ) -> Result<(), ConnectionDenied> {
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
            && (self.config.inbound_max_peers + self.config.inbound_overflowing_peers).saturating_sub(peers as u32) > 0;
        if peers >= limit as usize && !is_overflowing_inbound_connection {
            return Err(SlotConnectionError::LimitExceeded { limit, direction }.into());
        }

        let mut entry = match self.peers.entry(peer) {
            Entry::Occupied(entry) => {
                if let PeerState::JustDisconnected(_) = entry.get().state {
                    return Err(SlotConnectionError::ActiveBackoffPeriod.into());
                }

                entry
            }
            Entry::Vacant(entry) => entry.insert_entry(PeerEntry {
                connections: Default::default(),
                direction,
                state: PeerState::JustConnected,
                lowest_ping: Duration::MAX,
                connected_at: Instant::now(),
            }),
        };
        let entry = entry.get_mut();

        // TODO: we might want to check peer existed before
        if let PeerDirection::Inbound(direction) = &mut entry.direction
            && is_overflowing_inbound_connection
        {
            *direction = InboundPeerDirection::Overflowing {
                latest_action: None,
            };
        }

        entry.connections.insert(connection_id);

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
                let peer_entry = entry.get_mut();
                peer_entry.connections.remove(&connection_id);
                if peer_entry.connections.is_empty() {
                    // peer can be in Connected state if the grace period is ended
                    // peer can be in JustConnected state if the peer is blocked beforehand via peer scoring
                    debug_assert_matches!(
                        peer_entry.state,
                        PeerState::JustConnected | PeerState::Connected
                    );
                    peer_entry.state = PeerState::JustDisconnected(Instant::now());
                }

                true
            }
            Entry::Vacant(_) => false,
        }
    }

    fn update_periods(&mut self) {
        self.peers.retain(|_peer, entry| match entry.state {
            PeerState::JustConnected => {
                if entry.connected_at.elapsed() > self.config.grace_period {
                    entry.state = PeerState::Connected;
                }

                true
            }
            PeerState::Connected => true,
            PeerState::JustDisconnected(at) => at.elapsed() <= self.config.backoff_period,
        });
    }

    // TODO: close only inbound connections
    fn evict_inbound_overflowing_peers(&mut self) {
        let peers = self
            .inbound_peers()
            .filter(|(_peer, entry)| {
                entry.direction.is_evictable_overflowing_inbound(
                    self.config.inbound_overflowing_peer_action_timeout,
                )
            })
            .map(|(&peer, _entry)| peer)
            .collect::<Vec<_>>();

        for peer_id in peers {
            self.pending_events.push_back(ToSwarm::CloseConnection {
                peer_id,
                connection: CloseConnection::All,
            })
        }
    }

    fn dial_peers(&mut self) {
        let outbounds_peers = self.outbound_peers().count();
        let Some(needed_outbound_peers) = (self.config.outbound_min_peers as usize)
            .checked_sub(outbounds_peers)
            .and_then(|peers| peers.checked_sub(self.pending_outbound_peers.len()))
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

    fn on_driver_tick(&mut self) {
        self.update_periods();
        self.evict_inbound_overflowing_peers();
        self.dial_peers();
    }

    fn handle_ping_event(&mut self, event: ping::Event) {
        let ping::Event {
            peer,
            connection: _,
            result,
        } = event;

        let entry = self.peers.get_mut(&peer).expect("unknown peer");

        match result {
            Ok(ping) => {
                entry.lowest_ping = cmp::min(entry.lowest_ping, ping);
            }
            Err(err) => {
                // NOTE: the unsupported protocol is an error too
                log::debug!("disconnect peer {peer} on failed ping: {err}");
                self.pending_events.push_back(ToSwarm::CloseConnection {
                    peer_id: peer,
                    connection: CloseConnection::All,
                })
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler =
        ConnectionHandlerSelect<THandler<ping::Behaviour>, dummy::ConnectionHandler>;
    type ToSwarm = Infallible;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.ping
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let ping_handler = self.ping.handle_established_inbound_connection(
            connection_id,
            peer,
            local_addr,
            remote_addr,
        )?;
        self.add_inbound_connection(peer, connection_id)?;

        Ok(ping_handler.select(dummy::ConnectionHandler))
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.pending_outbound_peers.extend(maybe_peer);

        let ping_addresses = self.ping.handle_pending_outbound_connection(
            connection_id,
            maybe_peer,
            addresses,
            effective_role,
        )?;
        if self.outbound_peers().count() >= self.config.outbound_max_peers as usize {
            return Err(SlotConnectionError::LimitExceeded {
                limit: self.config.outbound_max_peers,
                direction: PeerDirection::Outbound,
            }
            .into());
        }

        Ok(ping_addresses)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        addr: &Multiaddr,
        role_override: Endpoint,
        port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        let ping_handler = self.ping.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            role_override,
            port_use,
        )?;
        self.pending_outbound_peers.remove(&peer);
        self.add_outbound_connection(peer, connection_id)?;

        Ok(ping_handler.select(dummy::ConnectionHandler))
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.ping.on_swarm_event(event);
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
                connection_id: _,
            }) => {
                self.pending_outbound_peers.remove(&peer_id);
            }
            _ => {}
        }
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        match event {
            Either::Left(event) => {
                self.ping
                    .on_connection_handler_event(peer_id, connection_id, event)
            }
            Either::Right(event) => match event {},
        }
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(to_swarm) = self.pending_events.pop_front() {
            return Poll::Ready(
                to_swarm
                    .map_in(|event| match event {})
                    .map_out(|event| match event {}),
            );
        }

        if let Poll::Ready(_instant) = self.driver.poll_tick(cx) {
            self.on_driver_tick();
        }

        if let Poll::Ready(to_swarm) = self.ping.poll(cx) {
            match to_swarm {
                ToSwarm::GenerateEvent(event) => self.handle_ping_event(event),
                to_swarm => {
                    return Poll::Ready(to_swarm.map_in(|event| match event {}).map_out(
                        |_event| unreachable!("`ToSwarm::GenerateEvent` is handled above"),
                    ));
                }
            };
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
}
