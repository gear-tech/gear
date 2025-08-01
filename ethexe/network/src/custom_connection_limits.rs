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

use crate::utils::ConnectionMap;
use libp2p::{
    Multiaddr, PeerId,
    core::{ConnectedPoint, Endpoint, transport::PortUse},
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler,
        THandlerInEvent, THandlerOutEvent, ToSwarm, dummy,
    },
};
use std::{
    convert::Infallible,
    task::{Context, Poll},
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, derive_more::Display)]
pub enum LimitExceededKind {
    #[display("established incoming per peer")]
    EstablishedIncomingPerPeer,
    #[display("established outbound per peer")]
    EstablishedOutboundPerPeer,
}

#[derive(Debug, Eq, PartialEq, derive_more::Display)]
#[display("custom connection limit exceeded: at most {limit} {kind} are allowed")]
pub struct LimitExceeded {
    pub limit: u32,
    pub kind: LimitExceededKind,
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

pub struct Behaviour {
    established_incoming_per_peer: ConnectionMap,
    established_outbound_per_peer: ConnectionMap,
}

impl Behaviour {
    pub fn new(limits: Limits) -> Self {
        Self {
            established_incoming_per_peer: ConnectionMap::new(
                limits.max_established_incoming_per_peer,
            ),
            established_outbound_per_peer: ConnectionMap::new(
                limits.max_established_outbound_per_peer,
            ),
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
        self.established_incoming_per_peer
            .add_connection(peer, connection_id)
            .map_err(|limit| {
                ConnectionDenied::new(LimitExceeded {
                    limit,
                    kind: LimitExceededKind::EstablishedIncomingPerPeer,
                })
            })?;

        Ok(dummy::ConnectionHandler)
    }

    fn handle_established_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        _addr: &Multiaddr,
        _role_override: Endpoint,
        _port_use: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.established_outbound_per_peer
            .add_connection(peer, connection_id)
            .map_err(|limit| {
                ConnectionDenied::new(LimitExceeded {
                    limit,
                    kind: LimitExceededKind::EstablishedOutboundPerPeer,
                })
            })?;

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
    use crate::utils::tests::init_logger;
    use libp2p::{
        Swarm,
        futures::{StreamExt, stream},
        swarm::{
            DialError, ListenError, SwarmEvent,
            dial_opts::{DialOpts, PeerCondition},
        },
    };
    use libp2p_swarm_test::SwarmExt;

    fn new_swarm(limits: Limits) -> Swarm<Behaviour> {
        SwarmExt::new_ephemeral_tokio(|_keypair| Behaviour::new(limits))
    }

    fn take_n_events<const NUM_EVENTS: usize>(
        swarm: &mut Swarm<Behaviour>,
    ) -> impl stream::Stream<Item = SwarmEvent<Infallible>> + '_ {
        stream::unfold(swarm, |swarm| async move {
            let event = swarm.next_swarm_event().await;
            Some((event, swarm))
        })
        .take(NUM_EVENTS)
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
