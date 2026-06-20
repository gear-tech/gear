// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

pub(crate) use libp2p::gossipsub::*;

use crate::{
    db_sync::{Multiaddr, PeerId},
    peer_score,
};
use anyhow::anyhow;
use bytes::Bytes;
use ethexe_common::{Address, injected::SignedCompactTxReceipt, network::SignedValidatorMessage};
use libp2p::{
    core::{Endpoint, transport::PortUse},
    gossipsub,
    identity::Keypair,
    metrics::Recorder,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, THandler, THandlerInEvent,
        THandlerOutEvent, ToSwarm,
    },
};
use parity_scale_codec::{Decode, Encode};
use seahash::SeaHasher;
use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    sync::Arc,
    task::{Context, Poll, ready},
    time::Duration,
};

#[derive(Debug)]
pub enum Message {
    // TODO: rename to `Validators`
    Commitments(SignedValidatorMessage),
    TxReceipt(SignedCompactTxReceipt),
    MalachiteConsensus(Bytes),
    MalachiteLiveness(Bytes),
    MalachiteProposalParts(Bytes),
}

impl From<SignedValidatorMessage> for Message {
    fn from(message: SignedValidatorMessage) -> Self {
        Self::Commitments(message)
    }
}

impl From<SignedCompactTxReceipt> for Message {
    fn from(message: SignedCompactTxReceipt) -> Self {
        Self::TxReceipt(message)
    }
}

impl Message {
    fn topic_hash(&self, behaviour: &Behaviour) -> TopicHash {
        match self {
            Message::Commitments(_) => behaviour.commitments_topic.hash(),
            Message::TxReceipt(_) => behaviour.tx_receipts_topic.hash(),
            Message::MalachiteConsensus(_) => behaviour.malachite_consensus_topic.hash(),
            Message::MalachiteLiveness(_) => behaviour.malachite_liveness_topic.hash(),
            Message::MalachiteProposalParts(_) => behaviour.malachite_proposal_parts_topic.hash(),
        }
    }

    fn encode(&self) -> Vec<u8> {
        match self {
            Message::Commitments(message) => message.encode(),
            Message::TxReceipt(message) => message.encode(),
            Message::MalachiteConsensus(data)
            | Message::MalachiteLiveness(data)
            | Message::MalachiteProposalParts(data) => data.to_vec(),
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
        source: PeerId,
        validator: MessageValidator,
    },
    PublishFailure {
        error: PublishError,
        message: Message,
        topic: TopicHash,
    },
}

pub(crate) struct Behaviour {
    inner: gossipsub::Behaviour,
    peer_score: peer_score::Handle,
    // TODO: consider to limit queue
    message_queue: VecDeque<Message>,
    commitments_topic: IdentTopic,
    tx_receipts_topic: IdentTopic,
    malachite_consensus_topic: IdentTopic,
    malachite_liveness_topic: IdentTopic,
    malachite_proposal_parts_topic: IdentTopic,
    metrics: Arc<libp2p::metrics::Metrics>,
}

impl Behaviour {
    pub fn new(
        keypair: Keypair,
        peer_score: peer_score::Handle,
        router_address: Address,
        malachite_config: &malachitebft_network::Config,
        registry: &mut libp2p::metrics::Registry,
        metrics: Arc<libp2p::metrics::Metrics>,
    ) -> anyhow::Result<Self> {
        let commitments_topic = Self::topic_with_router("commitments", router_address);
        let tx_receipts_topic = Self::topic_with_router("receipts", router_address);
        let malachite_consensus_topic =
            Self::malachite_topic_with_router("consensus", router_address);
        let malachite_liveness_topic =
            Self::malachite_topic_with_router("liveness", router_address);
        let malachite_proposal_parts_topic =
            Self::malachite_topic_with_router("proposal-parts", router_address);

        let inner = Self::build_config(
            malachite_config,
            [
                malachite_consensus_topic.hash(),
                malachite_liveness_topic.hash(),
                malachite_proposal_parts_topic.hash(),
            ],
        )?;
        let mut inner = gossipsub::Behaviour::new(MessageAuthenticity::Signed(keypair), inner)
            .map_err(|e| anyhow!("`gossipsub::Behaviour` error: {e}"))?
            .with_metrics(
                registry.sub_registry_with_prefix("libp2p_gossipsub"),
                MetricsConfig::default(),
            );
        if malachite_config.gossipsub.enable_peer_scoring {
            inner
                .with_peer_score(
                    malachitebft_network::peer_scoring::peer_score_params(),
                    malachitebft_network::peer_scoring::peer_score_thresholds(),
                )
                .map_err(|e| anyhow!("`gossipsub` scoring parameters error: {e}"))?;
        }

        inner.subscribe(&commitments_topic)?;
        inner.subscribe(&tx_receipts_topic)?;
        inner.subscribe(&malachite_consensus_topic)?;
        inner.subscribe(&malachite_liveness_topic)?;
        inner.subscribe(&malachite_proposal_parts_topic)?;

        Ok(Self {
            inner,
            peer_score,
            message_queue: VecDeque::new(),
            commitments_topic,
            tx_receipts_topic,
            malachite_consensus_topic,
            malachite_liveness_topic,
            malachite_proposal_parts_topic,
            metrics,
        })
    }

    fn topic_with_router(name: &'static str, router_address: Address) -> IdentTopic {
        IdentTopic::new(format!("{name}-{router_address}"))
    }

    fn malachite_topic_with_router(name: &'static str, router_address: Address) -> IdentTopic {
        IdentTopic::new(format!("malachite-{name}-{router_address}"))
    }

    fn build_config(
        config: &malachitebft_network::Config,
        malachite_topics: [TopicHash; 3],
    ) -> anyhow::Result<Config> {
        // These settings mirror malachitebft-network's private gossipsub builder.
        ConfigBuilder::default()
            .protocol_id_prefix("/ethexe/gossipsub/1.0.0")
            .max_transmit_size(config.pubsub_max_size)
            .opportunistic_graft_ticks(
                malachitebft_network::peer_scoring::OPPORTUNISTIC_GRAFT_TICKS,
            )
            .opportunistic_graft_peers(
                malachitebft_network::peer_scoring::OPPORTUNISTIC_GRAFT_PEERS,
            )
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .validate_messages()
            .history_gossip(3)
            .history_length(5)
            .mesh_n_high(config.gossipsub.mesh_n_high)
            .mesh_n_low(config.gossipsub.mesh_n_low)
            .mesh_outbound_min(config.gossipsub.mesh_outbound_min)
            .mesh_n(config.gossipsub.mesh_n)
            .flood_publish(config.gossipsub.enable_flood_publish)
            .message_id_fn(move |message| {
                let mut hasher = SeaHasher::new();
                if malachite_topics.iter().any(|topic| topic == &message.topic) {
                    message.hash(&mut hasher);
                } else {
                    message.topic.hash(&mut hasher);
                    message.data.hash(&mut hasher);
                }
                MessageId::new(hasher.finish().to_be_bytes().as_slice())
            })
            .build()
            .map_err(|e| anyhow!("`gossipsub::ConfigBuilder::build()` error: {e}"))
    }

    pub fn publish(&mut self, message: impl Into<Message>) {
        self.message_queue.push_back(message.into());
    }

    pub fn publish_malachite_consensus(&mut self, data: Bytes) {
        self.publish(Message::MalachiteConsensus(data));
    }

    pub fn publish_malachite_liveness(&mut self, data: Bytes) {
        self.publish(Message::MalachiteLiveness(data));
    }

    pub fn publish_malachite_proposal_part(&mut self, data: Bytes) {
        self.publish(Message::MalachiteProposalParts(data));
    }

    fn handle_inner_event(&mut self, event: gossipsub::Event) -> Poll<Event> {
        self.metrics.record(&event);

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

                let message = if topic == self.commitments_topic.hash() {
                    match SignedValidatorMessage::decode(&mut &data[..]).map(Message::Commitments) {
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
                    }
                } else if topic == self.tx_receipts_topic.hash() {
                    match SignedCompactTxReceipt::decode(&mut &data[..]).map(Message::TxReceipt) {
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
                    }
                } else if topic == self.malachite_consensus_topic.hash() {
                    Message::MalachiteConsensus(Bytes::from(data))
                } else if topic == self.malachite_liveness_topic.hash() {
                    Message::MalachiteLiveness(Bytes::from(data))
                } else if topic == self.malachite_proposal_parts_topic.hash() {
                    Message::MalachiteProposalParts(Bytes::from(data))
                } else {
                    log::trace!("received gossip message on unknown topic {topic:?}");
                    let validated = self.inner.report_message_validation_result(
                        &message_id,
                        &propagation_source,
                        MessageAcceptance::Ignore,
                    );
                    debug_assert!(validated);
                    return Poll::Pending;
                };

                let validator = MessageValidator {
                    message_id,
                    propagation_source,
                    message,
                };

                Poll::Ready(Event::Message { source, validator })
            }
            gossipsub::Event::Subscribed {
                peer_id: _,
                topic: _,
            } => Poll::Pending,
            gossipsub::Event::Unsubscribed {
                peer_id: _,
                topic: _,
            } => Poll::Pending,
            gossipsub::Event::GossipsubNotSupported { peer_id } => {
                log::trace!("peer doesn't support gossipsub: {peer_id}");
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

    #[cfg(test)]
    pub(crate) fn malachite_topic_hashes_for_tests(&self) -> MalachiteTopicHashes {
        MalachiteTopicHashes {
            consensus: self.malachite_consensus_topic.hash(),
            liveness: self.malachite_liveness_topic.hash(),
            proposal_parts: self.malachite_proposal_parts_topic.hash(),
        }
    }
}

#[cfg(test)]
pub(crate) struct MalachiteTopicHashes {
    pub consensus: TopicHash,
    pub liveness: TopicHash,
    pub proposal_parts: TopicHash,
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
                Err(PublishError::NoPeersSubscribedToTopic) => break,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_behaviour(router_address: Address) -> Behaviour {
        let mut registry = libp2p::metrics::Registry::default();
        let metrics = Arc::new(libp2p::metrics::Metrics::new(&mut registry));

        Behaviour::new(
            Keypair::generate_ed25519(),
            peer_score::Handle::new_test(),
            router_address,
            &crate::NetworkService::default_malachite_config(),
            &mut registry,
            metrics,
        )
        .expect("gossipsub behaviour builds")
    }

    fn received_message(behaviour: &mut Behaviour, topic: TopicHash, data: &[u8]) -> Message {
        let source = Keypair::generate_ed25519().public().to_peer_id();
        let event = gossipsub::Event::Message {
            propagation_source: source,
            message_id: MessageId::new(b"test-message"),
            message: gossipsub::Message {
                source: Some(source),
                data: data.to_vec(),
                sequence_number: Some(0),
                topic,
            },
        };

        let Poll::Ready(Event::Message { validator, .. }) = behaviour.handle_inner_event(event)
        else {
            panic!("expected gossipsub wrapper message");
        };

        validator.message
    }

    fn raw_gossipsub_message(
        topic: TopicHash,
        data: &[u8],
        sequence_number: u64,
    ) -> gossipsub::Message {
        gossipsub::Message {
            source: Some(Keypair::generate_ed25519().public().to_peer_id()),
            data: data.to_vec(),
            sequence_number: Some(sequence_number),
            topic,
        }
    }

    #[test]
    fn message_ids_keep_ethexe_payload_dedup_but_allow_malachite_restreams() {
        let behaviour = test_behaviour(Address::from([7u8; 20]));
        let config = Behaviour::build_config(
            &crate::NetworkService::default_malachite_config(),
            [
                behaviour.malachite_consensus_topic.hash(),
                behaviour.malachite_liveness_topic.hash(),
                behaviour.malachite_proposal_parts_topic.hash(),
            ],
        )
        .expect("gossipsub config builds");

        let ethexe_a =
            raw_gossipsub_message(behaviour.commitments_topic.hash(), b"same-payload", 1);
        let ethexe_b =
            raw_gossipsub_message(behaviour.commitments_topic.hash(), b"same-payload", 2);
        assert_eq!(config.message_id(&ethexe_a), config.message_id(&ethexe_b));

        let malachite_a = raw_gossipsub_message(
            behaviour.malachite_liveness_topic.hash(),
            b"same-payload",
            1,
        );
        let malachite_b = raw_gossipsub_message(
            behaviour.malachite_liveness_topic.hash(),
            b"same-payload",
            2,
        );
        assert_ne!(
            config.message_id(&malachite_a),
            config.message_id(&malachite_b)
        );
    }

    #[test]
    fn malachite_topics_route_to_raw_message_variants() {
        let mut behaviour = test_behaviour(Address::from([7u8; 20]));
        let data = Bytes::from_static(b"malachite-payload");

        let consensus_topic = behaviour.malachite_consensus_topic.hash();
        let liveness_topic = behaviour.malachite_liveness_topic.hash();
        let proposal_parts_topic = behaviour.malachite_proposal_parts_topic.hash();

        let message = received_message(&mut behaviour, consensus_topic, &data);
        assert!(matches!(message, Message::MalachiteConsensus(actual) if actual == data));

        let message = received_message(&mut behaviour, liveness_topic, &data);
        assert!(matches!(message, Message::MalachiteLiveness(actual) if actual == data));

        let message = received_message(&mut behaviour, proposal_parts_topic, &data);
        assert!(matches!(message, Message::MalachiteProposalParts(actual) if actual == data));
    }
}
