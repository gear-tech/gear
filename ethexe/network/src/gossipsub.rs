// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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
use anyhow::anyhow;
use libp2p::{
    core::{Endpoint, transport::PortUse},
    gossipsub,
    identity::Keypair,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use std::{
    collections::VecDeque,
    hash::{DefaultHasher, Hash, Hasher},
    task::{Context, Poll, ready},
};

use gprimitives::utils::ByteSliceFormatter;
pub(crate) use libp2p::gossipsub::*;

#[derive(derive_more::Debug)]
pub(crate) enum Event {
    Message {
        message_id: MessageId,
        propagation_source: PeerId,
        source: PeerId,
        #[debug("{:.8}", ByteSliceFormatter::Dynamic(data))]
        data: Vec<u8>,
        topic: TopicHash,
    },
    PublishFailure {
        error: PublishError,
        topic: TopicHash,
    },
    Subscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
    Unsubscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
    GossipsubNotSupported {
        peer_id: PeerId,
    },
}

pub(crate) struct Behaviour {
    inner: gossipsub::Behaviour,
    message_queue: VecDeque<(TopicHash, Vec<u8>)>,
}

impl Behaviour {
    pub fn new(keypair: Keypair) -> anyhow::Result<Self> {
        let inner = ConfigBuilder::default()
            // dedup messages
            .message_id_fn(|msg| {
                let mut hasher = DefaultHasher::new();
                msg.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_be_bytes())
            })
            .validation_mode(ValidationMode::Strict)
            .validate_messages()
            .build()
            .map_err(|e| anyhow!("`gossipsub::ConfigBuilder::build()` error: {e}"))?;
        let mut inner = gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair), inner)
            .map_err(|e| anyhow!("`gossipsub::Behaviour` error: {e}"))?;
        inner
            .with_peer_score(PeerScoreParams::default(), PeerScoreThresholds::default())
            .map_err(|e| anyhow!("`gossipsub` scoring parameters error: {e}"))?;

        Ok(Self {
            inner,
            message_queue: VecDeque::new(),
        })
    }

    pub fn publish(&mut self, topic: impl Into<TopicHash>, data: Vec<u8>) {
        self.message_queue.push_back((topic.into(), data));
    }

    pub fn subscribe(&mut self, topic: &IdentTopic) -> Result<bool, SubscriptionError> {
        self.inner.subscribe(topic)
    }

    pub fn report_message_validation_result(
        &mut self,
        msg_id: &MessageId,
        propagation_source: &PeerId,
        acceptance: MessageAcceptance,
    ) -> bool {
        self.inner
            .report_message_validation_result(msg_id, propagation_source, acceptance)
    }

    fn handle_inner_event(&self, event: gossipsub::Event) -> Poll<Event> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message_id,
                message:
                    Message {
                        source,
                        data,
                        sequence_number: _,
                        topic,
                    },
            } => {
                let source =
                    source.expect("ValidationMode::Strict implies `source` is always present");

                Poll::Ready(Event::Message {
                    message_id,
                    propagation_source,
                    source,
                    data,
                    topic,
                })
            }
            gossipsub::Event::Subscribed { peer_id, topic } => {
                Poll::Ready(Event::Subscribed { peer_id, topic })
            }
            gossipsub::Event::Unsubscribed { peer_id, topic } => {
                Poll::Ready(Event::Unsubscribed { peer_id, topic })
            }
            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                Poll::Ready(Event::GossipsubNotSupported { peer_id })
            }
            gossipsub::Event::SlowPeer {
                peer_id,
                failed_messages: _,
            } => {
                // TODO: consider to score peer
                log::trace!("SlowPeer received {peer_id}");
                Poll::Pending
            }
        }
    }
}

impl NetworkBehaviour for Behaviour {
    type ConnectionHandler = <gossipsub::Behaviour as NetworkBehaviour>::ConnectionHandler;
    type ToSwarm = Event;

    fn handle_pending_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        local_addr: &Multiaddr,
        remote_addr: &Multiaddr,
    ) -> Result<(), ConnectionDenied> {
        self.inner
            .handle_pending_inbound_connection(connection_id, local_addr, remote_addr)
    }

    fn handle_established_inbound_connection(
        &mut self,
        connection_id: ConnectionId,
        peer: PeerId,
        localaddr: &Multiaddr,
        remoteaddr: &Multiaddr,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner
            .handle_established_inbound_connection(connection_id, peer, localaddr, remoteaddr)
    }

    fn handle_pending_outbound_connection(
        &mut self,
        connection_id: ConnectionId,
        maybe_peer: Option<PeerId>,
        addresses: &[Multiaddr],
        effective_role: Endpoint,
    ) -> Result<Vec<Multiaddr>, ConnectionDenied> {
        self.inner.handle_pending_outbound_connection(
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
        roleoverride: Endpoint,
        portuse: PortUse,
    ) -> Result<THandler<Self>, ConnectionDenied> {
        self.inner.handle_established_outbound_connection(
            connection_id,
            peer,
            addr,
            roleoverride,
            portuse,
        )
    }

    fn on_swarm_event(&mut self, event: FromSwarm) {
        self.inner.on_swarm_event(event)
    }

    fn on_connection_handler_event(
        &mut self,
        peer_id: PeerId,
        connection_id: ConnectionId,
        event: THandlerOutEvent<Self>,
    ) {
        self.inner
            .on_connection_handler_event(peer_id, connection_id, event)
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        if let Some((topic, data)) = self.message_queue.front() {
            match self.inner.publish(topic.clone(), data.clone()) {
                Ok(_msg_id) => {
                    let _ = self.message_queue.pop_front().expect("checked above");
                }
                Err(PublishError::InsufficientPeers) => {}
                Err(error) => {
                    let (topic, _data) = self.message_queue.pop_front().expect("checked above");
                    return Poll::Ready(ToSwarm::GenerateEvent(Event::PublishFailure {
                        error,
                        topic,
                    }));
                }
            }
        }

        let to_swarm = ready!(self.inner.poll(cx));
        match to_swarm {
            ToSwarm::GenerateEvent(event) => {
                self.handle_inner_event(event).map(ToSwarm::GenerateEvent)
            }
            to_swarm => Poll::Ready(to_swarm.map_out::<Event>(|_event| {
                unreachable!("`ToSwarm::GenerateEvent` is handled above")
            })),
        }
    }
}
