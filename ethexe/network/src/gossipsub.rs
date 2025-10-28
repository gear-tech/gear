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

pub(crate) use libp2p::gossipsub::*;

use crate::{
    db_sync::{Multiaddr, PeerId},
    peer_score,
};
use anyhow::anyhow;
use ethexe_common::{Address, network::SignedValidatorMessage, tx_pool::SignedOffchainTransaction};
use libp2p::{
    core::{Endpoint, transport::PortUse},
    gossipsub,
    identity::Keypair,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::VecDeque,
    hash::{DefaultHasher, Hash, Hasher},
    task::{Context, Poll, ready},
};

#[derive(Debug, derive_more::From)]
pub enum Message {
    Commitments(SignedValidatorMessage),
    Offchain(SignedOffchainTransaction),
}

impl Message {
    fn topic_hash(&self, behaviour: &Behaviour) -> TopicHash {
        match self {
            Message::Commitments(_) => behaviour.commitments_topic.hash(),
            Message::Offchain(_) => behaviour.offchain_topic.hash(),
        }
    }

    fn encode(&self) -> Vec<u8> {
        match self {
            Message::Commitments(message) => message.encode(),
            Message::Offchain(transaction) => transaction.encode(),
        }
    }
}

#[derive(Debug)]
pub struct MessageValidator {
    message_id: MessageId,
    propagation_source: PeerId,
    message: Message,
}

impl MessageValidator {
    pub(crate) fn validate<F, T>(self, behaviour: &mut Behaviour, f: F) -> T
    where
        F: FnOnce(Message) -> (MessageAcceptance, T),
    {
        let Self {
            message_id,
            propagation_source,
            message,
        } = self;
        let (acceptance, message) = f(message);
        let validated = behaviour.inner.report_message_validation_result(
            &message_id,
            &propagation_source,
            acceptance,
        );
        debug_assert!(validated);
        message
    }
}

#[derive(derive_more::Debug)]
pub(crate) enum Event {
    Message {
        // will be used in the future
        #[allow(dead_code)]
        source: PeerId,
        validator: MessageValidator,
    },
    PublishFailure {
        error: PublishError,
        message: Message,
        topic: TopicHash,
    },
    Subscribed {
        peer_id: PeerId,
        topic: TopicHash,
    },
}

pub(crate) struct Behaviour {
    inner: gossipsub::Behaviour,
    peer_score: peer_score::Handle,
    // TODO: consider to limit queue
    message_queue: VecDeque<Message>,
    commitments_topic: IdentTopic,
    offchain_topic: IdentTopic,
}

impl Behaviour {
    pub fn new(
        keypair: Keypair,
        peer_score: peer_score::Handle,
        router_address: Address,
    ) -> anyhow::Result<Self> {
        let commitments_topic = Self::topic_with_router("commitments", router_address);
        let offchain_topic = Self::topic_with_router("offchain", router_address);

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
        inner.subscribe(&commitments_topic)?;
        inner.subscribe(&offchain_topic)?;

        Ok(Self {
            inner,
            peer_score,
            message_queue: VecDeque::new(),
            commitments_topic,
            offchain_topic,
        })
    }

    pub fn topic_with_router(name: &'static str, router_address: Address) -> IdentTopic {
        IdentTopic::new(format!("{name}-{router_address}"))
    }

    pub fn publish(&mut self, message: impl Into<Message>) {
        self.message_queue.push_back(message.into());
    }

    fn handle_inner_event(&mut self, event: gossipsub::Event) -> Poll<Event> {
        match event {
            gossipsub::Event::Message {
                propagation_source,
                message_id,
                message:
                    gossipsub::Message {
                        source,
                        data,
                        sequence_number: _,
                        topic,
                    },
            } => {
                let source =
                    source.expect("ValidationMode::Strict implies `source` is always present");

                let res = if topic == self.commitments_topic.hash() {
                    SignedValidatorMessage::decode(&mut &data[..]).map(Message::Commitments)
                } else if topic == self.offchain_topic.hash() {
                    SignedOffchainTransaction::decode(&mut &data[..]).map(Message::Offchain)
                } else {
                    unreachable!("topic we never subscribed to: {topic:?}");
                };

                let message = match res {
                    Ok(message) => message,
                    Err(error) => {
                        log::trace!("failed to decode gossip message from {source}: {error}");
                        let validated = self.inner.report_message_validation_result(
                            &message_id,
                            &propagation_source,
                            MessageAcceptance::Reject,
                        );
                        debug_assert!(validated);
                        self.peer_score.invalid_data(source);
                        return Poll::Pending;
                    }
                };

                let validator = MessageValidator {
                    message_id,
                    propagation_source,
                    message,
                };

                Poll::Ready(Event::Message { source, validator })
            }
            gossipsub::Event::Subscribed { peer_id, topic } => {
                Poll::Ready(Event::Subscribed { peer_id, topic })
            }
            gossipsub::Event::Unsubscribed {
                peer_id: _,
                topic: _,
            } => Poll::Pending,
            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                log::trace!("peer doesn't support gossipsub: {peer_id}");
                self.peer_score.unsupported_protocol(peer_id);
                Poll::Pending
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
        while let Some(message) = self.message_queue.front() {
            let topic = message.topic_hash(self);
            let data = message.encode();

            match self.inner.publish(topic.clone(), data) {
                Ok(_msg_id) => {
                    let _ = self.message_queue.pop_front().expect("checked above");
                }
                Err(PublishError::InsufficientPeers) => break,
                Err(error) => {
                    let message = self.message_queue.pop_front().expect("checked above");
                    return Poll::Ready(ToSwarm::GenerateEvent(Event::PublishFailure {
                        error,
                        message,
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
