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

use crate::{
    db_sync::{Multiaddr, PeerId},
    utils::{ConnectionMap, PeerLimit, PeerLimitError},
};
use libp2p::{
    core::{ConnectedPoint, Endpoint, transport::PortUse},
    swarm::{
        ConnectionClosed, ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour,
        NewExternalAddrOfPeer, THandler, THandlerInEvent, THandlerOutEvent, ToSwarm,
        dial_opts::DialOpts, dummy,
    },
};
use std::{
    collections::VecDeque,
    convert::Infallible,
    task::{Context, Poll},
};

pub struct Config {
    inbound_peers: u32,
    outbound_peers: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            inbound_peers: 25,
            outbound_peers: 75,
        }
    }
}

#[derive(Debug, derive_more::Display, Eq, PartialEq)]
pub enum LimitExceededKind {
    #[display("inbound peers")]
    InboundPeers,
    #[display("outbound peers")]
    OutboundPeers,
}

#[derive(Debug, derive_more::Display)]
#[display("limit of {limit} {kind} exceeded")]
pub struct LimitExceeded {
    limit: u32,
    kind: LimitExceededKind,
}

impl std::error::Error for LimitExceeded {}

impl From<LimitExceeded> for ConnectionDenied {
    fn from(value: LimitExceeded) -> Self {
        ConnectionDenied::new(value)
    }
}

pub struct Behaviour {
    config: Config,
    inbound_peers: ConnectionMap<PeerLimit>,
    outbound_peers: ConnectionMap<PeerLimit>,
    pending_events: VecDeque<ToSwarm<Infallible, Infallible>>,
}

impl Behaviour {
    pub fn new(config: Config) -> Self {
        Self {
            inbound_peers: ConnectionMap::with_peer_limit(config.inbound_peers),
            outbound_peers: ConnectionMap::with_peer_limit(config.outbound_peers),
            config,
            pending_events: VecDeque::new(),
        }
    }

    fn check_inbound_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.inbound_peers
            .add_connection(peer, connection_id)
            .map_err(|PeerLimitError { limit }| LimitExceeded {
                limit,
                kind: LimitExceededKind::InboundPeers,
            })?;

        Ok(())
    }

    fn check_outbound_connection(
        &mut self,
        peer: PeerId,
        connection_id: ConnectionId,
    ) -> Result<(), ConnectionDenied> {
        self.outbound_peers
            .add_connection(peer, connection_id)
            .map_err(|PeerLimitError { limit }| LimitExceeded {
                limit,
                kind: LimitExceededKind::OutboundPeers,
            })?;

        Ok(())
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
        self.check_inbound_connection(peer, connection_id)?;
        Ok(dummy::ConnectionHandler)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        _addresses: &[Multiaddr],
        _effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        let Some(peer) = maybe_peer else {
            return Ok(vec![]);
        };

        self.check_outbound_connection(peer, connection_id)?;

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
        self.check_outbound_connection(peer, connection_id)?;
        Ok(dummy::ConnectionHandler)
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        match event {
            FromSwarm::ConnectionClosed(ConnectionClosed {
                peer_id,
                connection_id,
                endpoint,
                cause: _,
                remaining_established: _,
            }) => match endpoint {
                ConnectedPoint::Dialer { .. } => {
                    self.outbound_peers
                        .remove_connection(peer_id, connection_id);
                }
                ConnectedPoint::Listener { .. } => {
                    self.inbound_peers.remove_connection(peer_id, connection_id);
                }
            },
            FromSwarm::NewExternalAddrOfPeer(NewExternalAddrOfPeer { peer_id, addr }) => {
                if self.outbound_peers.peers().len() >= self.config.outbound_peers as usize {
                    return;
                }

                self.pending_events.push_back(ToSwarm::Dial {
                    opts: DialOpts::peer_id(peer_id)
                        .addresses(vec![addr.clone()])
                        .extend_addresses_through_behaviour()
                        .build(),
                });
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
        _cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some(swarm) = self.pending_events.pop_front() {
            return Poll::Ready(swarm);
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
        let alice_addr = alice.external_addresses().next().cloned().unwrap();

        for _ in 0..Config::default().inbound_peers {
            let mut peer = new_swarm().await;
            peer.connect(&mut alice).await;
            tokio::spawn(peer.loop_on_next());
        }

        let mut bob = new_swarm().await;
        bob.add_peer_address(alice_peer_id, alice_addr);
        let bob_peer_id = *bob.local_peer_id();
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
            let err = cause.downcast::<LimitExceeded>().unwrap();
            assert_eq!(err.limit, Config::default().inbound_peers);
            assert_eq!(err.kind, LimitExceededKind::InboundPeers);
        } else {
            unreachable!("unexpected event: {event:?}");
        }
    }

    #[tokio::test]
    async fn outbound_peers_limit() {
        init_logger();

        let mut alice = new_swarm().await;

        for _ in 0..Config::default().outbound_peers {
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
        let err = cause.downcast::<LimitExceeded>().unwrap();
        assert_eq!(err.limit, Config::default().outbound_peers);
        assert_eq!(err.kind, LimitExceededKind::OutboundPeers);
    }
}
