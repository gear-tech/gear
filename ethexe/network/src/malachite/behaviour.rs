use anyhow::anyhow;
use bytes::Bytes;
use libp2p::{PeerId, request_response, swarm::NetworkBehaviour};
use libp2p_broadcast as broadcast;
use malachitebft_network::{self as network, validator_proof};
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
    pub broadcast: broadcast::Behaviour,
    pub sync: sync::Behaviour,
    pub validator_proof: validator_proof::Behaviour,
}

impl Behaviour {
    pub(crate) fn new(
        config: &network::Config,
        registry: &mut libp2p::metrics::Registry,
    ) -> anyhow::Result<Self> {
        let mut broadcast = broadcast::Behaviour::new_with_metrics(
            broadcast::Config {
                max_buf_size: config.pubsub_max_size,
            },
            registry.sub_registry_with_prefix("malachite_broadcast"),
        );
        broadcast.subscribe(network::Channel::Sync.to_broadcast_topic(config.channel_names));

        let sync = sync::Behaviour::new(
            sync::Config::default().with_max_response_size(config.rpc_max_size),
            config.protocol_names.sync.clone(),
        )
        .map_err(|e| anyhow!("`malachite sync behaviour` error: {e:?}"))?;

        let validator_proof_protocol =
            libp2p::StreamProtocol::try_from_owned(config.protocol_names.validator_proof.clone())?;
        let validator_proof = validator_proof::Behaviour::new(validator_proof_protocol);

        Ok(Self {
            broadcast,
            sync,
            validator_proof,
        })
    }

    pub(crate) fn broadcast_status(&mut self, data: Bytes) {
        let topic = network::Channel::Sync.to_broadcast_topic(network::ChannelNames::default());
        self.broadcast.broadcast(&topic, data);
    }

    pub(crate) fn send_sync_request(
        &mut self,
        peer: PeerId,
        body: Bytes,
    ) -> request_response::OutboundRequestId {
        self.sync.send_request(peer, body)
    }

    pub(crate) fn send_sync_response(&mut self, channel: ResponseChannel, body: Bytes) {
        if let Err(error) = self.sync.send_response(channel, body) {
            log::warn!("failed to send Malachite sync response: {error}");
        }
    }
}
