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

use libp2p::{
    core::{ConnectedPoint, Endpoint},
    swarm::{
        dummy, ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
    },
    Multiaddr, PeerId,
};
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    fmt,
    task::{Context, Poll},
};
use void::Void;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum LimitExceededKind {
    EstablishedIncomingPerPeer,
    EstablishedOutboundPerPeer,
}

impl fmt::Display for LimitExceededKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LimitExceededKind::EstablishedIncomingPerPeer => {
                f.write_str("established incoming per peer")
            }
            LimitExceededKind::EstablishedOutboundPerPeer => {
                f.write_str("established outbound per peer")
            }
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct LimitExceeded {
    pub limit: u32,
    pub kind: LimitExceededKind,
}

impl fmt::Display for LimitExceeded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "custom connection limit exceeded: at most {} {} are allowed",
            self.limit, self.kind
        )
    }
}

impl std::error::Error for LimitExceeded {}

#[derive(Default)]
pub struct Limits {
    pub max_established_incoming_per_peer: Option<u32>,
    pub max_established_outbound_per_peer: Option<u32>,
}

impl Limits {
    pub fn with_max_established_incoming_per_peer(mut self, limit: Option<u32>) -> Self {
        self.max_established_incoming_per_peer = limit;
        self
    }

    pub fn with_max_established_outbound_per_peer(mut self, limit: Option<u32>) -> Self {
        self.max_established_outbound_per_peer = limit;
        self
    }
}

#[derive(Debug)]
struct ConnectionMap {
    inner: HashMap<PeerId, HashSet<ConnectionId>>,
    limit: Option<u32>,
    kind: LimitExceededKind,
}

impl ConnectionMap {
    fn new(limit: Option<u32>, kind: LimitExceededKind) -> Self {
        Self {
            inner: Default::default(),
            limit,
            kind,
        }
    }

    fn check_limit(&self, peer_id: PeerId) -> Result<(), ConnectionDenied> {
        let current = self
            .inner
            .get(&peer_id)
            .map(|connections| connections.len())
            .unwrap_or(0) as u32;
        let limit = self.limit.unwrap_or(u32::MAX);
        if current < limit {
            Ok(())
        } else {
            Err(ConnectionDenied::new(LimitExceeded {
                limit,
                kind: self.kind,
            }))
        }
    }

    fn add_connection(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.check_limit(peer_id)?;
        self.inner.entry(peer_id).or_default().insert(connection_id);
        Ok(())
    }

    fn remove_connection(&mut self, peer_id: PeerId, connection_id: ConnectionId) {
        if let Entry::Occupied(mut entry) = self.inner.entry(peer_id) {
            let connections = entry.get_mut();
            connections.remove(&connection_id);

            if connections.is_empty() {
                entry.remove();
            }
        }
    }
}

pub struct Behaviour {
    established_incoming_per_peer: ConnectionMap,
    established_outbound_per_peer: ConnectionMap,
}

impl Behaviour {
    pub fn new(limits: Limits) -> Self {
        Self {
            established_incoming_per_peer: ConnectionMap::new(
                limits.max_established_incoming_per_peer,
                LimitExceededKind::EstablishedIncomingPerPeer,
            ),
            established_outbound_per_peer: ConnectionMap::new(
                limits.max_established_outbound_per_peer,
                LimitExceededKind::EstablishedOutboundPerPeer,
            ),
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = dummy::ConnectionHandler;
    type ToSwarm = Void;

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _local_addr: &Multiaddr,
        _remote_addr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.established_incoming_per_peer
            .add_connection(peer, connection_id)?;

        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.established_outbound_per_peer
            .add_connection(peer, connection_id)?;

        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        if let FromSwarm::ConnectionClosed(ConnectionClosed {
            peer_id,
            connection_id,
            endpoint,
            ..
        }) = event
        {
            match endpoint {
                ConnectedPoint::Dialer { .. } => {
                    self.established_outbound_per_peer
                        .remove_connection(peer_id, connection_id);
                }
                ConnectedPoint::Listener { .. } => {
                    self.established_incoming_per_peer
                        .remove_connection(peer_id, connection_id);
                }
            }
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
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::{
        futures::{stream, StreamExt},
        swarm::{
            dial_opts::{DialOpts, PeerCondition},
            DialError, ListenError, SwarmEvent,
        },
        Swarm,
    };
    use libp2p_swarm_test::SwarmExt;

    fn init_logger() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    fn new_swarm(limits: Limits) -> Swarm<Behaviour> {
        SwarmExt::new_ephemeral(|_keypair| Behaviour::new(limits))
    }

    fn take_n_events<const NUM_EVENTS: usize>(
        swarm: &mut Swarm<Behaviour>,
    ) -> impl stream::Stream<Item = SwarmEvent<Void>> + '_ {
        stream::unfold(swarm, |swarm| async move {
            let event = swarm.next_swarm_event().await;
            Some((event, swarm))
        })
        .take(NUM_EVENTS)
    }

    #[test]
    fn connection_map_limit_works() {
        const LIMIT: u32 = 5;

        let mut map =
            ConnectionMap::new(Some(LIMIT), LimitExceededKind::EstablishedIncomingPerPeer);

        let main_peer = PeerId::random();

        for i in 0..LIMIT {
            map.add_connection(main_peer, ConnectionId::new_unchecked(i as usize))
                .unwrap();
        }

        let err = map
            .add_connection(main_peer, ConnectionId::new_unchecked(usize::MAX))
            .unwrap_err();
        assert_eq!(
            *err.downcast_ref::<LimitExceeded>().unwrap(),
            LimitExceeded {
                limit: LIMIT,
                kind: LimitExceededKind::EstablishedIncomingPerPeer
            }
        );

        // new peer so no limit exceeded yet
        map.add_connection(
            PeerId::random(),
            ConnectionId::new_unchecked(usize::MAX / 2),
        )
        .unwrap();
    }

    #[test]
    fn connection_map_key_cleared() {
        let mut map = ConnectionMap::new(None, LimitExceededKind::EstablishedIncomingPerPeer);

        let peer_set: HashSet<PeerId> = [
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
            PeerId::random(),
        ]
        .into();
        let new_connection_id = |i, j| ConnectionId::new_unchecked(i * (j as usize + 10));

        for (i, &peer) in peer_set.iter().enumerate() {
            for j in 0..10 {
                map.add_connection(peer, new_connection_id(i, j)).unwrap();
            }
        }

        assert_eq!(
            map.inner.clone().into_keys().collect::<HashSet<PeerId>>(),
            peer_set
        );

        for (i, &peer) in peer_set.iter().enumerate() {
            for j in 0..10 {
                map.remove_connection(peer, new_connection_id(i, j));
            }
        }

        assert_eq!(
            map.inner.into_keys().collect::<HashSet<PeerId>>(),
            Default::default()
        );
    }

    #[tokio::test]
    async fn inbound_connection_denied() {
        const PEERS: usize = 10;
        const INBOUND_CONNECTIONS: usize = 10;
        const INBOUND_LIMIT: u32 = 5;

        init_logger();

        let mut limited_peer = new_swarm(
            Limits::default().with_max_established_incoming_per_peer(Some(INBOUND_LIMIT)),
        );
        let limited_peer_id = *limited_peer.local_peer_id();
        let (limited_peer_addr, _) = limited_peer.listen().with_memory_addr_external().await;

        let mut unlimited_peers = vec![];
        for _ in 0..PEERS {
            let mut peer = new_swarm(Limits::default());
            let peer_id = *peer.local_peer_id();

            for _ in 0..INBOUND_CONNECTIONS {
                peer.dial(
                    DialOpts::peer_id(limited_peer_id)
                        .condition(PeerCondition::Always)
                        .addresses(vec![limited_peer_addr.clone()])
                        .build(),
                )
                .unwrap();
            }

            unlimited_peers.push(peer_id);

            tokio::spawn(peer.loop_on_next());
        }

        for _ in 0..PEERS {
            take_n_events::<INBOUND_CONNECTIONS>(&mut limited_peer)
                .for_each(|event| async move {
                    assert!(matches!(event, SwarmEvent::IncomingConnection { .. }));
                })
                .await;
        }

        for unlimited_peer_id in unlimited_peers {
            // first `INBOUND_LIMIT` connections are established
            take_n_events::<{ INBOUND_LIMIT as usize }>(&mut limited_peer)
                .for_each(|event| async move {
                    assert!(matches!(event, SwarmEvent::ConnectionEstablished { peer_id,.. } if peer_id == unlimited_peer_id));
                })
                .await;

            // the rest of connections are denied
            take_n_events::<{ INBOUND_CONNECTIONS - INBOUND_LIMIT as usize }>(&mut limited_peer)
                .for_each(|event| async move {
                    if let SwarmEvent::IncomingConnectionError {
                        error: ListenError::Denied { cause },
                        ..
                    } = event
                    {
                        let exceeded = cause.downcast::<LimitExceeded>().unwrap();
                        assert_eq!(
                            exceeded,
                            LimitExceeded {
                                limit: 5,
                                kind: LimitExceededKind::EstablishedIncomingPerPeer,
                            }
                        );
                    } else {
                        unreachable!("{event:?}");
                    }
                })
                .await;
        }
    }

    #[tokio::test]
    async fn outbound_connection_denied() {
        const PEERS: usize = 10;
        const OUTBOUND_CONNECTIONS: usize = 10;
        const OUTBOUND_LIMIT: u32 = 5;

        init_logger();

        let mut limited_peer = new_swarm(
            Limits::default().with_max_established_outbound_per_peer(Some(OUTBOUND_LIMIT)),
        );

        let mut unlimited_peers = vec![];
        for _ in 0..PEERS {
            let mut peer = new_swarm(Limits::default());
            let (peer_addr, _) = peer.listen().with_memory_addr_external().await;

            let peer_id = *peer.local_peer_id();
            for _ in 0..OUTBOUND_CONNECTIONS {
                limited_peer
                    .dial(
                        DialOpts::peer_id(peer_id)
                            .condition(PeerCondition::Always)
                            .addresses(vec![peer_addr.clone()])
                            .build(),
                    )
                    .unwrap();
            }

            tokio::spawn(peer.loop_on_next());

            unlimited_peers.push(peer_id);
        }

        for unlimited_peer_id in unlimited_peers {
            // first `OUTBOUND_LIMIT` connections are established
            take_n_events::<{ OUTBOUND_LIMIT as usize }>(&mut limited_peer)
                .for_each(|event| async move {
                    assert!(matches!(event, SwarmEvent::ConnectionEstablished { peer_id,.. } if peer_id == unlimited_peer_id));
                })
                .await;

            // the rest of connections are denied
            take_n_events::<{ OUTBOUND_CONNECTIONS - OUTBOUND_LIMIT as usize }>(&mut limited_peer)
                .for_each(|event| async move {
                    if let SwarmEvent::OutgoingConnectionError {
                        error: DialError::Denied { cause },
                        peer_id: Some(peer_id),
                        ..
                    } = event
                    {
                        let exceeded = cause.downcast::<LimitExceeded>().unwrap();
                        assert_eq!(
                            exceeded,
                            LimitExceeded {
                                limit: 5,
                                kind: LimitExceededKind::EstablishedOutboundPerPeer,
                            }
                        );
                        assert_eq!(peer_id, unlimited_peer_id);
                    } else {
                        unreachable!("{event:?}");
                    }
                })
                .await;
        }
    }
}
