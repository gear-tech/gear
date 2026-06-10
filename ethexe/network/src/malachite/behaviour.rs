use std::{
    hash::{Hash, Hasher},
    time::Duration,
};

use anyhow::anyhow;
use bytes::Bytes;
use libp2p::{
    PeerId, gossipsub, identity, request_response,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
};
use libp2p_broadcast as broadcast;
use malachitebft_network::{self as network, PubSubProtocol, validator_proof};
use malachitebft_sync::{self as sync, ResponseChannel};
use seahash::SeaHasher;

const OPPORTUNISTIC_GRAFT_THRESHOLD: f64 = 100_000.0;
const OPPORTUNISTIC_GRAFT_TICKS: u64 = 3;
const OPPORTUNISTIC_GRAFT_PEERS: usize = 2;
const APP_SPECIFIC_WEIGHT: f64 = 100.0;

#[derive(Debug)]
pub(crate) enum Event {
    GossipSub(gossipsub::Event),
    Broadcast(broadcast::Event),
    Sync(sync::Event),
    ValidatorProof(validator_proof::Event),
}

impl From<gossipsub::Event> for Event {
    fn from(event: gossipsub::Event) -> Self {
        Self::GossipSub(event)
    }
}

impl From<broadcast::Event> for Event {
    fn from(event: broadcast::Event) -> Self {
        Self::Broadcast(event)
    }
}

impl From<sync::Event> for Event {
    fn from(event: sync::Event) -> Self {
        Self::Sync(event)
    }
}

impl From<validator_proof::Event> for Event {
    fn from(event: validator_proof::Event) -> Self {
        Self::ValidatorProof(event)
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "Event")]
pub(crate) struct Behaviour {
    pub gossipsub: Toggle<gossipsub::Behaviour>,
    pub broadcast: Toggle<broadcast::Behaviour>,
    pub sync: Toggle<sync::Behaviour>,
    pub validator_proof: Toggle<validator_proof::Behaviour>,
}

impl Behaviour {
    pub(crate) fn new(
        keypair: identity::Keypair,
        config: &network::Config,
        proof_bytes: Option<Bytes>,
        registry: &mut libp2p::metrics::Registry,
    ) -> anyhow::Result<Self> {
        let mut gossipsub = if config.enable_consensus
            && matches!(config.pubsub_protocol, PubSubProtocol::GossipSub)
        {
            Some(new_gossipsub(keypair, config, registry)?)
        } else {
            None
        };

        let mut broadcast = if config.enable_sync
            || (config.enable_consensus
                && matches!(config.pubsub_protocol, PubSubProtocol::Broadcast))
        {
            Some(broadcast::Behaviour::new_with_metrics(
                broadcast::Config {
                    max_buf_size: config.pubsub_max_size,
                },
                registry.sub_registry_with_prefix("malachite_broadcast"),
            ))
        } else {
            None
        };

        if config.enable_consensus {
            subscribe_consensus_channels(&mut gossipsub, &mut broadcast, config)?;
        }

        if config.enable_sync
            && let Some(broadcast) = broadcast.as_mut()
        {
            broadcast.subscribe(network::Channel::Sync.to_broadcast_topic(config.channel_names));
        }

        let sync = if config.enable_sync {
            Some(
                sync::Behaviour::new(
                    sync::Config::default().with_max_response_size(config.rpc_max_size),
                    config.protocol_names.sync.clone(),
                )
                .map_err(|e| anyhow!("`malachite sync behaviour` error: {e:?}"))?,
            )
        } else {
            None
        };

        let validator_proof = if config.enable_consensus {
            let protocol = libp2p::StreamProtocol::try_from_owned(
                config.protocol_names.validator_proof.clone(),
            )?;
            let mut behaviour = validator_proof::Behaviour::new(protocol);
            if let Some(proof_bytes) = proof_bytes {
                behaviour.set_proof(proof_bytes);
            }
            Some(behaviour)
        } else {
            None
        };

        Ok(Self {
            gossipsub: Toggle::from(gossipsub),
            broadcast: Toggle::from(broadcast),
            sync: Toggle::from(sync),
            validator_proof: Toggle::from(validator_proof),
        })
    }

    pub(crate) fn publish_consensus(&mut self, data: Bytes) {
        self.publish_pubsub(network::Channel::Consensus, data);
    }

    pub(crate) fn publish_liveness(&mut self, data: Bytes) {
        self.publish_pubsub(network::Channel::Liveness, data);
    }

    pub(crate) fn publish_proposal_part(&mut self, data: Bytes) {
        self.publish_pubsub(network::Channel::ProposalParts, data);
    }

    pub(crate) fn broadcast_status(&mut self, data: Bytes) {
        if let Some(broadcast) = self.broadcast.as_mut() {
            let topic = network::Channel::Sync.to_broadcast_topic(network::ChannelNames::default());
            broadcast.broadcast(&topic, data);
        }
    }

    pub(crate) fn send_sync_request(
        &mut self,
        peer: PeerId,
        body: Bytes,
    ) -> request_response::OutboundRequestId {
        self.sync
            .as_mut()
            .expect("Malachite sync behaviour registered")
            .send_request(peer, body)
    }

    pub(crate) fn send_sync_response(&mut self, channel: ResponseChannel, body: Bytes) {
        if let Err(error) = self
            .sync
            .as_mut()
            .expect("Malachite sync behaviour registered")
            .send_response(channel, body)
        {
            log::warn!("failed to send Malachite sync response: {error}");
        }
    }

    fn publish_pubsub(&mut self, channel: network::Channel, data: Bytes) {
        if let Some(gossipsub) = self.gossipsub.as_mut() {
            let topic = channel.to_gossipsub_topic(network::ChannelNames::default());
            if let Err(error) = gossipsub.publish(topic, data) {
                log::warn!("failed to publish Malachite {channel} message: {error}");
            }
        }
    }
}

fn subscribe_consensus_channels(
    gossipsub: &mut Option<gossipsub::Behaviour>,
    broadcast: &mut Option<broadcast::Behaviour>,
    config: &network::Config,
) -> anyhow::Result<()> {
    match config.pubsub_protocol {
        PubSubProtocol::GossipSub => {
            if let Some(gossipsub) = gossipsub.as_mut() {
                for channel in network::Channel::consensus() {
                    gossipsub.subscribe(&channel.to_gossipsub_topic(config.channel_names))?;
                }
            }
        }
        PubSubProtocol::Broadcast => {
            if let Some(broadcast) = broadcast.as_mut() {
                for channel in network::Channel::consensus() {
                    broadcast.subscribe(channel.to_broadcast_topic(config.channel_names));
                }
            }
        }
    }

    Ok(())
}

fn new_gossipsub(
    keypair: identity::Keypair,
    config: &network::Config,
    registry: &mut libp2p::metrics::Registry,
) -> anyhow::Result<gossipsub::Behaviour> {
    let gossipsub_config = gossipsub::ConfigBuilder::default()
        .protocol_id_prefix(config.protocol_names.consensus.clone())
        .max_transmit_size(config.pubsub_max_size)
        .opportunistic_graft_ticks(OPPORTUNISTIC_GRAFT_TICKS)
        .opportunistic_graft_peers(OPPORTUNISTIC_GRAFT_PEERS)
        .heartbeat_interval(Duration::from_secs(1))
        .validation_mode(gossipsub::ValidationMode::Strict)
        .history_gossip(3)
        .history_length(5)
        .mesh_n_high(config.gossipsub.mesh_n_high)
        .mesh_n_low(config.gossipsub.mesh_n_low)
        .mesh_outbound_min(config.gossipsub.mesh_outbound_min)
        .mesh_n(config.gossipsub.mesh_n)
        .flood_publish(config.gossipsub.enable_flood_publish)
        .message_id_fn(message_id)
        .build()
        .map_err(|e| anyhow!("`malachite gossipsub config` error: {e}"))?;

    let mut behaviour = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair),
        gossipsub_config,
    )
    .map_err(|e| anyhow!("`malachite gossipsub behaviour` error: {e}"))?;

    if config.gossipsub.enable_peer_scoring {
        behaviour
            .with_peer_score(peer_score_params(), peer_score_thresholds())
            .map_err(|e| anyhow!("`malachite gossipsub peer scoring` error: {e}"))?;
    }

    let behaviour = behaviour.with_metrics(
        registry.sub_registry_with_prefix("malachite_gossipsub"),
        Default::default(),
    );

    Ok(behaviour)
}

fn message_id(message: &gossipsub::Message) -> gossipsub::MessageId {
    let mut hasher = SeaHasher::new();
    message.hash(&mut hasher);
    gossipsub::MessageId::new(hasher.finish().to_be_bytes().as_slice())
}

fn peer_score_params() -> gossipsub::PeerScoreParams {
    gossipsub::PeerScoreParams {
        app_specific_weight: APP_SPECIFIC_WEIGHT,
        ..Default::default()
    }
}

fn peer_score_thresholds() -> gossipsub::PeerScoreThresholds {
    gossipsub::PeerScoreThresholds {
        opportunistic_graft_threshold: OPPORTUNISTIC_GRAFT_THRESHOLD,
        gossip_threshold: -500.0,
        publish_threshold: -1000.0,
        graylist_threshold: -2000.0,
        ..Default::default()
    }
}
