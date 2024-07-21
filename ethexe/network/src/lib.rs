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
    futures::StreamExt,
    gossipsub, identify, identity, kad, mdns,
    multiaddr::Protocol,
    ping, request_response,
    request_response::{Event, Message},
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        NetworkBehaviour, SwarmEvent,
    },
    Multiaddr, PeerId, StreamProtocol, Swarm, SwarmBuilder,
};
use parity_scale_codec::Decode;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    str::FromStr,
};
use tokio::{select, sync::mpsc};

pub const DEFAULT_LISTEN_PORT: u16 = 20333;

pub const PROTOCOL_VERSION: &str = "ethexe/0.1.0";
pub const AGENT_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

const MAX_ESTABLISHED_INCOMING_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_OUTBOUND_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_INCOMING_CONNECTIONS: u32 = 100;

const DB_SYNC_STREAM_PROTOCOL: StreamProtocol =
    StreamProtocol::new(concat!("/ethexe/db-sync/", env!("CARGO_PKG_VERSION")));

pub struct NetworkService {
    pub sender: NetworkSender,
    pub receiver: NetworkReceiver,
    pub event_loop: NetworkEventLoop,
}

impl NetworkService {
    pub fn new(config: NetworkEventLoopConfig, signer: &Signer) -> anyhow::Result<NetworkService> {
        fs::create_dir_all(&config.config_dir)
            .context("failed to create network configuration directory")?;

        let keypair =
            NetworkEventLoop::generate_keypair(signer, &config.config_dir, config.public_key)?;
        let mut swarm = NetworkEventLoop::create_swarm(keypair)?;

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

        let (network_sender, sender_rx) = NetworkSender::new();
        let (receiver_tx, network_receiver) = NetworkReceiver::new();

        Ok(Self {
            sender: network_sender,
            receiver: network_receiver,
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
    RequestDb {},
}

#[derive(Debug, Clone)]
pub struct NetworkSender {
    tx: mpsc::UnboundedSender<NetworkSenderEvent>,
}

impl NetworkSender {
    fn new() -> (Self, mpsc::UnboundedReceiver<NetworkSenderEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub fn publish_commitments(&self, data: impl Into<Vec<u8>>) {
        let _res = self
            .tx
            .send(NetworkSenderEvent::PublishCommitments { data: data.into() });
    }

    pub fn request_db(&self) {
        let _res = self.tx.send(NetworkSenderEvent::RequestDb {});
    }
}

#[derive(Debug)]
pub enum NetworkReceiverEvent {
    Commitments {
        source: Option<PeerId>,
        data: Vec<u8>,
    },
}

pub struct NetworkReceiver {
    rx: mpsc::UnboundedReceiver<NetworkReceiverEvent>,
}

impl NetworkReceiver {
    fn new() -> (mpsc::UnboundedSender<NetworkReceiverEvent>, Self) {
        let (tx, rx) = mpsc::unbounded_channel();
        (tx, Self { rx })
    }

    pub async fn recv(&mut self) -> Option<NetworkReceiverEvent> {
        self.rx.recv().await
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DbSyncRequest {}

#[derive(Debug, Serialize, Deserialize)]
pub struct DbSyncResponse {}

#[derive(Debug, Clone)]
pub struct NetworkEventLoopConfig {
    pub config_dir: PathBuf,
    pub public_key: Option<PublicKey>,
    pub external_addresses: HashSet<Multiaddr>,
    pub bootstrap_addresses: HashSet<Multiaddr>,
    pub listen_addresses: HashSet<Multiaddr>,
}

impl NetworkEventLoopConfig {
    pub fn new_local(config_path: PathBuf) -> Self {
        Self {
            config_dir: config_path,
            public_key: None,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: ["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()].into(),
        }
    }
}

pub struct NetworkEventLoop {
    swarm: Swarm<Behaviour>,
    rx: mpsc::UnboundedReceiver<NetworkSenderEvent>,
    tx: mpsc::UnboundedSender<NetworkReceiverEvent>,
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

    fn create_swarm(keypair: identity::Keypair) -> anyhow::Result<Swarm<Behaviour>> {
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_quic()
            .with_behaviour(Behaviour::from_keypair)?
            .build();
        Ok(swarm)
    }

    pub async fn run(mut self) {
        loop {
            select! {
                event = self.swarm.select_next_some() => self.handle_swarm_event(event),
                event = self.rx.recv() => match event {
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
                message:
                    gossipsub::Message {
                        source,
                        data,
                        sequence_number: _,
                        topic,
                    },
                ..
            }) if gpu_commitments_topic().hash() == topic => {
                self.tx
                    .send(NetworkReceiverEvent::Commitments { source, data })
                    .expect("channel dropped unexpectedly");
            }
            BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                message:
                    gossipsub::Message {
                        source,
                        data,
                        sequence_number: _,
                        topic,
                    },
                ..
            }) if DbSyncTopicMessage::ident_topic().hash() == topic => {
                let _message = match DbSyncTopicMessage::decode(&mut data.as_slice()) {
                    Ok(m) => m,
                    Err(err) => {
                        log::debug!("invalid db-sync topic message: {err}");
                        return;
                    }
                };
            }
            BehaviourEvent::Gossipsub(gossipsub::Event::GossipsubNotSupported { peer_id }) => {
                log::debug!("`gossipsub` protocol is not supported. Disconnecting...");
                let _res = self.swarm.disconnect_peer_id(peer_id);
            }
            BehaviourEvent::Gossipsub(_) => {}
            //
            BehaviourEvent::DbSync(e) => match e {
                Event::Message { peer, message } => match message {
                    Message::Request {
                        request_id,
                        request,
                        channel,
                    } => {
                        let DbSyncRequest {} = request;
                        if let Err(_resp) = self
                            .swarm
                            .behaviour_mut()
                            .db_sync
                            .send_response(channel, DbSyncResponse {})
                        {
                            log::debug!("failed to send response for {peer} peer and {request_id} request: channel is closed");
                        }
                    }
                    Message::Response {
                        request_id,
                        response,
                    } => {}
                },
                Event::OutboundFailure { .. } => {}
                Event::InboundFailure { .. } => {}
                Event::ResponseSent { .. } => {}
            },
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
            NetworkSenderEvent::RequestDb {} => {}
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
    //
    pub db_sync: request_response::json::Behaviour<DbSyncRequest, DbSyncResponse>,
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

        let db_sync = request_response::json::Behaviour::new(
            [(
                DB_SYNC_STREAM_PROTOCOL,
                request_response::ProtocolSupport::Full,
            )],
            request_response::Config::default(),
        );

        Ok(Self {
            custom_connection_limits,
            connection_limits,
            ping,
            identify,
            mdns4,
            kad,
            gossipsub,
            db_sync,
        })
    }
}

fn gpu_commitments_topic() -> gossipsub::IdentTopic {
    // TODO: use router address in topic name to avoid obsolete router
    gossipsub::IdentTopic::new("gpu-commitments")
}

#[derive(Decode)]
struct DbSyncTopicMessage {}

impl DbSyncTopicMessage {
    fn ident_topic() -> gossipsub::IdentTopic {
        gossipsub::IdentTopic::new("db-sync")
    }
}
