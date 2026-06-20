use anyhow::anyhow;
use bytes::Bytes;
use libp2p::{
    PeerId, identity, request_response,
    swarm::{NetworkBehaviour, behaviour::toggle::Toggle},
};
use libp2p_broadcast as broadcast;
use malachitebft_network::{self as network, PubSubProtocol, validator_proof};
use malachitebft_sync::{self as sync, ResponseChannel};

#[derive(Debug)]
pub(crate) enum Event {
    Broadcast(broadcast::Event),
    Sync(sync::Event),
    ValidatorProof(validator_proof::Event),
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
    pub broadcast: Toggle<broadcast::Behaviour>,
    pub sync: Toggle<sync::Behaviour>,
    pub validator_proof: Toggle<validator_proof::Behaviour>,
}

impl Behaviour {
    pub(crate) fn new(
        _keypair: identity::Keypair,
        config: &network::Config,
        proof_bytes: Option<Bytes>,
        registry: &mut libp2p::metrics::Registry,
    ) -> anyhow::Result<Self> {
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

        if config.enable_consensus
            && matches!(config.pubsub_protocol, PubSubProtocol::Broadcast)
            && let Some(broadcast) = broadcast.as_mut()
        {
            for channel in network::Channel::consensus() {
                broadcast.subscribe(channel.to_broadcast_topic(config.channel_names));
            }
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
            broadcast: Toggle::from(broadcast),
            sync: Toggle::from(sync),
            validator_proof: Toggle::from(validator_proof),
        })
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
}
