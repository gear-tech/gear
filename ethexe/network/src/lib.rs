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

mod custom_connection_limits;

pub mod utils {
    pub use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
}

use anyhow::Context;
use ethexe_signer::{PublicKey, Signer};
use libp2p::{
    connection_limits,
    core::upgrade,
    futures::{Stream, StreamExt},
    gossipsub, identify, identity, kad, mdns,
    multiaddr::Protocol,
    ping,
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        Config as SwarmConfig, NetworkBehaviour, SwarmEvent,
    },
    Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
};
use std::{
    collections::HashSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    task::Poll,
};
use tokio::{select, sync::mpsc};

pub const DEFAULT_LISTEN_PORT: u16 = 20333;

pub const PROTOCOL_VERSION: &str = "ethexe/0.1.0";
pub const AGENT_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

const MAX_ESTABLISHED_INCOMING_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_OUTBOUND_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_INCOMING_CONNECTIONS: u32 = 100;

pub struct NetworkService {
    pub sender: NetworkSender,
    pub gossip_stream: GossipsubMessageStream,
    pub event_loop: NetworkEventLoop,
}

impl NetworkService {
    pub fn new(config: NetworkEventLoopConfig, signer: &Signer) -> anyhow::Result<NetworkService> {
        fs::create_dir_all(&config.config_dir)
            .context("failed to create network configuration directory")?;

        let keypair =
            NetworkEventLoop::generate_keypair(signer, &config.config_dir, config.public_key)?;
        let mut swarm = NetworkEventLoop::create_swarm(keypair, config.transport_type)?;

        for multiaddr in config.external_addresses {
            swarm.add_external_address(multiaddr);
        }

        for multiaddr in config.listen_addresses {
            swarm.listen_on(multiaddr).context("`listen_on()` failed")?;
        }

        for multiaddr in config.bootstrap_addresses {
            let peer_id = multiaddr
                .iter()
                .find_map(|p| {
                    if let Protocol::P2p(peer_id) = p {
                        Some(peer_id)
                    } else {
                        None
                    }
                })
                .context("bootstrap nodes are not allowed without peer ID")?;

            swarm.behaviour_mut().kad.add_address(&peer_id, multiaddr);
        }

        let (general_tx, general_rx) = mpsc::unbounded_channel();
        let (gossipsub_tx, gossipsub_rx) = mpsc::unbounded_channel();

        Ok(Self {
            sender: NetworkSender { tx: general_tx },
            gossip_stream: GossipsubMessageStream { rx: gossipsub_rx },
            event_loop: NetworkEventLoop {
                swarm,
                general_rx,
                gossipsub_tx,
            },
        })
    }
}

#[derive(Debug)]
enum NetworkSenderEvent {
    PublishCommitments { data: Vec<u8> },
}

/// Communication with [`NetworkEventLoop`]
#[derive(Debug, Clone)]
pub struct NetworkSender {
    tx: mpsc::UnboundedSender<NetworkSenderEvent>,
}

impl NetworkSender {
    pub fn publish_commitments(&self, data: impl Into<Vec<u8>>) {
        let _res = self
            .tx
            .send(NetworkSenderEvent::PublishCommitments { data: data.into() });
    }
}

#[derive(Debug)]
pub struct GossipsubMessage {
    pub source: Option<PeerId>,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct GossipsubMessageStream {
    rx: mpsc::UnboundedReceiver<GossipsubMessage>,
}

impl Stream for GossipsubMessageStream {
    type Item = GossipsubMessage;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_recv(cx)
    }
}

#[derive(Default, Debug, Clone)]
pub enum TransportType {
    #[default]
    Quic,
    Memory,
}

#[derive(Debug, Clone)]
pub struct NetworkEventLoopConfig {
    pub config_dir: PathBuf,
    pub public_key: Option<PublicKey>,
    pub external_addresses: HashSet<Multiaddr>,
    pub bootstrap_addresses: HashSet<Multiaddr>,
    pub listen_addresses: HashSet<Multiaddr>,
    pub transport_type: TransportType,
}

impl NetworkEventLoopConfig {
    pub fn new_local(config_path: PathBuf) -> Self {
        Self {
            config_dir: config_path,
            public_key: None,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: ["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()].into(),
            transport_type: TransportType::Quic,
        }
    }

    #[cfg(test)]
    pub fn new_memory(config_path: PathBuf, addr: &str) -> Self {
        Self {
            config_dir: config_path,
            public_key: None,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: [addr.parse().unwrap()].into(),
            transport_type: TransportType::Memory,
        }
    }
}

pub struct NetworkEventLoop {
    swarm: Swarm<Behaviour>,
    general_rx: mpsc::UnboundedReceiver<NetworkSenderEvent>,
    gossipsub_tx: mpsc::UnboundedSender<GossipsubMessage>,
}

impl NetworkEventLoop {
    fn generate_keypair(
        signer: &Signer,
        config_path: &Path,
        public_key: Option<PublicKey>,
    ) -> anyhow::Result<identity::Keypair> {
        let key = if let Some(key) = public_key {
            log::trace!("use networking key from command-line arguments");
            key
        } else {
            let public_key_path = config_path.join("public_key");
            if public_key_path.exists() {
                log::trace!("use networking key saved on disk");
                let key = fs::read_to_string(public_key_path)
                    .context("failed to read networking public key")?;
                PublicKey::from_str(&key)?
            } else {
                log::trace!("generate a new networking key");
                let key = signer.generate_key()?;
                fs::write(public_key_path, key.to_hex())
                    .context("failed to write networking public key")?;
                key
            }
        };

        let mut key = signer.get_private_key(key)?;
        let key = identity::secp256k1::SecretKey::try_from_bytes(&mut key.0)
            .expect("Signer provided invalid key; qed");
        let pair = identity::secp256k1::Keypair::from(key);
        Ok(identity::Keypair::from(pair))
    }

    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }

    fn create_swarm(
        keypair: identity::Keypair,
        transport_type: TransportType,
    ) -> anyhow::Result<Swarm<Behaviour>> {
        match transport_type {
            TransportType::Quic => Ok(SwarmBuilder::with_existing_identity(keypair)
                .with_tokio()
                .with_quic()
                .with_behaviour(Behaviour::from_keypair)?
                .build()),

            TransportType::Memory => {
                let transport = libp2p::core::transport::MemoryTransport::default()
                    .upgrade(upgrade::Version::V1)
                    .authenticate(libp2p::plaintext::Config::new(&keypair))
                    .multiplex(libp2p::yamux::Config::default())
                    .boxed();
                let behaviour =
                    Behaviour::from_keypair(&keypair).map_err(|err| anyhow::anyhow!(err))?;
                let config = SwarmConfig::with_tokio_executor()
                    .with_substream_upgrade_protocol_override(upgrade::Version::V1);

                Ok(Swarm::new(
                    transport,
                    behaviour,
                    keypair.public().to_peer_id(),
                    config,
                ))
            }
        }
    }

    pub async fn run(mut self) {
        loop {
            select! {
                event = self.swarm.select_next_some() => self.handle_swarm_event(event),
                event = self.general_rx.recv() => match event {
                    Some(event) => {
                        self.handle_network_rx_event(event);
                    }
                    None => {
                        log::info!("Network channel has been disconnected, shutting down network service...");
                        break;
                    },
                },
            }
        }
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        log::trace!("new swarm event: {event:?}");

        #[allow(clippy::single_match)]
        match event {
            SwarmEvent::Behaviour(e) => self.handle_behaviour_event(e),
            _ => {}
        }
    }

    fn handle_behaviour_event(&mut self, event: BehaviourEvent) {
        match event {
            BehaviourEvent::CustomConnectionLimits(void) => void::unreachable(void),
            //
            BehaviourEvent::ConnectionLimits(void) => void::unreachable(void),
            //
            BehaviourEvent::Ping(ping::Event {
                peer,
                connection: _,
                result,
            }) => {
                if let Err(e) = result {
                    log::debug!("Ping to {peer} failed: {e}. Disconnecting...");
                    let _res = self.swarm.disconnect_peer_id(peer);
                }
            }
            //
            BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                if info.protocol_version != PROTOCOL_VERSION || info.agent_version != AGENT_VERSION
                {
                    log::debug!("{peer_id} is not supported with `{}` protocol and `{}` agent. Disconnecting...", info.protocol_version, info.agent_version);
                    let _res = self.swarm.disconnect_peer_id(peer_id);
                }

                let behaviour = self.swarm.behaviour_mut();

                // add listen addresses of new peers to KadDHT
                // according to `identify` and `kad` protocols docs
                for listen_addr in info.listen_addrs {
                    behaviour.kad.add_address(&peer_id, listen_addr);
                }
            }
            BehaviourEvent::Identify(identify::Event::Error { peer_id, error, .. }) => {
                log::debug!("{peer_id} is not identified: {error}. Disconnecting...");
                let _res = self.swarm.disconnect_peer_id(peer_id);
            }
            BehaviourEvent::Identify(_) => {}
            //
            BehaviourEvent::Mdns4(mdns::Event::Discovered(peers)) => {
                for (peer_id, multiaddr) in peers {
                    if let Err(e) = self.swarm.dial(
                        DialOpts::peer_id(peer_id)
                            .condition(PeerCondition::Disconnected)
                            .addresses(vec![multiaddr])
                            .extend_addresses_through_behaviour()
                            .build(),
                    ) {
                        log::error!("dialing failed for mDNS address: {e:?}");
                    }
                }
            }
            BehaviourEvent::Mdns4(mdns::Event::Expired(peers)) => {
                for (peer_id, _multiaddr) in peers {
                    let _res = self.swarm.disconnect_peer_id(peer_id);
                }
            }
            //
            BehaviourEvent::Kad(kad::Event::RoutingUpdated { peer, .. }) => {
                let behaviour = self.swarm.behaviour_mut();
                if behaviour.mdns4.discovered_nodes().any(|&p| p == peer) {
                    // we don't want local peers to appear in KadDHT.
                    // event can be emitted few times in a row for
                    // the same peer, so we just ignore `None`
                    let _res = behaviour.kad.remove_peer(&peer);
                }
            }
            BehaviourEvent::Kad(_) => {}
            //
            BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message: gossipsub::Message { source, data, .. },
                ..
            }) => {
                let _res = self.gossipsub_tx.send(GossipsubMessage { source, data });
            }
            BehaviourEvent::Gossipsub(gossipsub::Event::GossipsubNotSupported { peer_id }) => {
                log::debug!("`gossipsub` protocol is not supported. Disconnecting...");
                let _res = self.swarm.disconnect_peer_id(peer_id);
            }
            BehaviourEvent::Gossipsub(_) => {}
        }
    }

    fn handle_network_rx_event(&mut self, event: NetworkSenderEvent) {
        match event {
            NetworkSenderEvent::PublishCommitments { data } => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gpu_commitments_topic(), data)
                {
                    log::debug!("gossipsub publishing failed: {e}")
                }
            }
        }
    }
}

#[derive(NetworkBehaviour)]
pub struct Behaviour {
    // custom options to limit connections
    pub custom_connection_limits: custom_connection_limits::Behaviour,
    // limit connections
    pub connection_limits: connection_limits::Behaviour,
    // fast peer liveliness check
    pub ping: ping::Behaviour,
    // friend or foe system
    pub identify: identify::Behaviour,
    // local discovery for IPv4 only
    pub mdns4: mdns::tokio::Behaviour,
    // global traversal discovery
    // TODO: consider to cache records in fs
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    // general communication
    pub gossipsub: gossipsub::Behaviour,
}

impl Behaviour {
    fn from_keypair(
        keypair: &identity::Keypair,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let peer_id = keypair.public().to_peer_id();

        // we use custom behaviour because
        // `libp2p::connection_limits::Behaviour` limits inbound & outbound
        // connections per peer in total, so protocols may fail to establish
        // at least 1 inbound & 1 outbound connection in specific circumstances
        // (for example, active VPN connection + communication with mDNS discovered peers)
        let custom_connection_limits = custom_connection_limits::Limits::default()
            .with_max_established_incoming_per_peer(Some(
                MAX_ESTABLISHED_INCOMING_PER_PEER_CONNECTIONS,
            ))
            .with_max_established_outbound_per_peer(Some(
                MAX_ESTABLISHED_OUTBOUND_PER_PEER_CONNECTIONS,
            ));
        let custom_connection_limits =
            custom_connection_limits::Behaviour::new(custom_connection_limits);

        let connection_limits = connection_limits::ConnectionLimits::default()
            .with_max_established_incoming(Some(MAX_ESTABLISHED_INCOMING_CONNECTIONS));
        let connection_limits = connection_limits::Behaviour::new(connection_limits);

        let ping = ping::Behaviour::default();

        let identify_config = identify::Config::new(PROTOCOL_VERSION.to_string(), keypair.public())
            .with_agent_version(AGENT_VERSION.to_string());
        let identify = identify::Behaviour::new(identify_config);

        let mdns4 = mdns::Behaviour::new(mdns::Config::default(), peer_id)?;

        let mut kad = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
        kad.set_mode(Some(kad::Mode::Server));

        let gossip_config = gossipsub::ConfigBuilder::default()
            // dedup messages
            .message_id_fn(|msg| {
                let mut hasher = DefaultHasher::new();
                msg.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_be_bytes())
            })
            .build()
            .map_err(|e| anyhow::anyhow!("`gossipsub::ConfigBuilder::build()` error: {e}"))?;
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossip_config,
        )
        .map_err(|e| anyhow::anyhow!("`gossipsub::Behaviour` error: {e}"))?;
        gossipsub
            .with_peer_score(
                gossipsub::PeerScoreParams::default(),
                gossipsub::PeerScoreThresholds::default(),
            )
            .map_err(|e| anyhow::anyhow!("`gossipsub` scoring parameters error: {e}"))?;

        gossipsub.subscribe(&gpu_commitments_topic())?;

        Ok(Self {
            custom_connection_limits,
            connection_limits,
            ping,
            identify,
            mdns4,
            kad,
            gossipsub,
        })
    }
}

fn gpu_commitments_topic() -> gossipsub::IdentTopic {
    // TODO: use router address in topic name to avoid obsolete router
    gossipsub::IdentTopic::new("gpu-commitments")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_memory_transport() {
        let _ = env_logger::builder().is_test(true).try_init();

        let tmp_dir1 = tempfile::tempdir().unwrap();
        let config = NetworkEventLoopConfig::new_memory(tmp_dir1.path().to_path_buf(), "/memory/1");
        let signer1 = ethexe_signer::Signer::new(tmp_dir1.path().join("key")).unwrap();
        let service1 = NetworkService::new(config.clone(), &signer1).unwrap();

        let peer_id = service1.event_loop.local_peer_id().to_string();

        let multiaddr: Multiaddr = format!("/memory/1/p2p/{}", peer_id).parse().unwrap();

        let (sender, mut _service1_handle) =
            (service1.sender, tokio::spawn(service1.event_loop.run()));

        // second service
        let tmp_dir2 = tempfile::tempdir().unwrap();
        let signer2 = ethexe_signer::Signer::new(tmp_dir2.path().join("key")).unwrap();
        let mut config2 =
            NetworkEventLoopConfig::new_memory(tmp_dir2.path().to_path_buf(), "/memory/2");

        config2.bootstrap_addresses = [multiaddr].into();

        let service2 = NetworkService::new(config2.clone(), &signer2).unwrap();

        tokio::spawn(service2.event_loop.run());

        // Wait for the connection to be established
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Send a commitment from service1
        let commitment_data = b"test commitment".to_vec();

        sender.publish_commitments(commitment_data.clone());

        let mut gossip_stream = service2.gossip_stream;

        // Wait for the commitment to be received by service2
        let received_commitment = timeout(Duration::from_secs(5), async {
            while let Some(message) = gossip_stream.next().await {
                if message.data == commitment_data {
                    return Some(message);
                }
            }

            None
        })
        .await
        .expect("Timeout while waiting for commitment");

        assert!(received_commitment.is_some(), "Commitment was not received");
    }
}
