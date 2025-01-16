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

mod custom_connection_limits;
pub mod db_sync;
pub mod peer_score;
mod utils;

pub mod export {
    pub use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
}

use anyhow::{anyhow, Context};
use ethexe_db::Database;
use ethexe_signer::{PublicKey, Signer};
use futures::future::Either;
use libp2p::{
    connection_limits,
    core::{muxing::StreamMuxerBox, upgrade},
    futures::StreamExt,
    gossipsub, identify, identity, kad, mdns,
    multiaddr::Protocol,
    ping,
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        Config as SwarmConfig, NetworkBehaviour, SwarmEvent,
    },
    yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::{
    collections::HashSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
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
    pub receiver: NetworkReceiver,
    pub event_loop: NetworkEventLoop,
}

impl NetworkService {
    pub fn new(
        config: NetworkEventLoopConfig,
        signer: &Signer,
        db: Database,
    ) -> anyhow::Result<NetworkService> {
        fs::create_dir_all(&config.config_dir)
            .context("failed to create network configuration directory")?;

        let keypair =
            NetworkEventLoop::generate_keypair(signer, &config.config_dir, config.public_key)?;
        let mut swarm = NetworkEventLoop::create_swarm(keypair, db, config.transport_type)?;

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
                external_rx: sender_rx,
                external_tx: receiver_tx,
            },
        })
    }
}

#[derive(Debug)]
enum NetworkSenderEvent {
    PublishMessage { data: Vec<u8> },
    RequestDbData(db_sync::Request),
    RequestValidated(Result<db_sync::ValidatingResponse, db_sync::ValidatingResponse>),
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

    // TODO: consider to append salt here to be sure that message is unique.
    // This is important for the cases of malfunctions in ethexe, when the same message
    // needs to be sent again #4255
    pub fn publish_message(&self, data: impl Into<Vec<u8>>) {
        let _res = self
            .tx
            .send(NetworkSenderEvent::PublishMessage { data: data.into() });
    }

    pub fn request_db_data(&self, request: db_sync::Request) {
        let _res = self.tx.send(NetworkSenderEvent::RequestDbData(request));
    }

    pub fn request_validated(
        &self,
        res: Result<db_sync::ValidatingResponse, db_sync::ValidatingResponse>,
    ) {
        let _res = self.tx.send(NetworkSenderEvent::RequestValidated(res));
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum NetworkReceiverEvent {
    Message {
        source: Option<PeerId>,
        data: Vec<u8>,
    },
    DbResponse(Result<db_sync::Response, db_sync::RequestFailure>),
    PeerBlocked(PeerId),
    ExternalValidation(db_sync::ValidatingResponse),
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

#[derive(Default, Debug, Clone)]
pub enum TransportType {
    #[default]
    QuicOrTcp,
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
            transport_type: TransportType::QuicOrTcp,
        }
    }

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
    // event receiver from service
    external_rx: mpsc::UnboundedReceiver<NetworkSenderEvent>,
    // event sender to service
    external_tx: mpsc::UnboundedSender<NetworkReceiverEvent>,
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

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    pub fn score_handle(&self) -> peer_score::Handle {
        self.swarm.behaviour().peer_score.handle()
    }

    fn create_swarm(
        keypair: identity::Keypair,
        db: Database,
        transport_type: TransportType,
    ) -> anyhow::Result<Swarm<Behaviour>> {
        let transport = match transport_type {
            TransportType::QuicOrTcp => {
                let tcp = libp2p::tcp::tokio::Transport::default()
                    .upgrade(upgrade::Version::V1Lazy)
                    .authenticate(libp2p::tls::Config::new(&keypair)?)
                    .multiplex(yamux::Config::default())
                    .timeout(Duration::from_secs(20));

                let quic_config = libp2p::quic::Config::new(&keypair);
                let quic = libp2p::quic::tokio::Transport::new(quic_config);

                quic.or_transport(tcp)
                    .map(|either_output, _| match either_output {
                        Either::Left((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
                        Either::Right((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
                    })
                    .boxed()
            }
            TransportType::Memory => libp2p::core::transport::MemoryTransport::default()
                .upgrade(upgrade::Version::V1Lazy)
                .authenticate(libp2p::plaintext::Config::new(&keypair))
                .multiplex(yamux::Config::default())
                .boxed(),
        };

        let behaviour = Behaviour::new(&keypair, db)?;
        let local_peer_id = keypair.public().to_peer_id();
        let config = SwarmConfig::with_tokio_executor();

        Ok(Swarm::new(transport, behaviour, local_peer_id, config))
    }

    pub async fn run(mut self) {
        loop {
            select! {
                event = self.swarm.select_next_some() => self.handle_swarm_event(event),
                event = self.external_rx.recv() => match event {
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
            BehaviourEvent::PeerScore(peer_score::Event::PeerBlocked {
                peer_id,
                last_reason: _,
            }) => {
                let _res = self
                    .external_tx
                    .send(NetworkReceiverEvent::PeerBlocked(peer_id));
            }
            BehaviourEvent::PeerScore(_) => {}
            //
            BehaviourEvent::Ping(ping::Event {
                peer,
                connection: _,
                result,
            }) => {
                if let Err(e) = result {
                    log::debug!("ping to {peer} failed: {e}. Disconnecting...");
                    let _res = self.swarm.disconnect_peer_id(peer);
                }
            }
            //
            BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                let behaviour = self.swarm.behaviour_mut();

                if info.protocol_version != PROTOCOL_VERSION || info.agent_version != AGENT_VERSION
                {
                    log::debug!(
                        "{peer_id} is not supported with `{}` protocol and `{}` agent",
                        info.protocol_version,
                        info.agent_version
                    );
                    behaviour.peer_score.handle().unsupported_protocol(peer_id);
                }

                // add listen addresses of new peers to KadDHT
                // according to `identify` and `kad` protocols docs
                for listen_addr in info.listen_addrs {
                    behaviour.kad.add_address(&peer_id, listen_addr);
                }
            }
            BehaviourEvent::Identify(identify::Event::Error { peer_id, error, .. }) => {
                log::debug!("{peer_id} is not identified: {error}");
                self.swarm
                    .behaviour()
                    .peer_score
                    .handle()
                    .unsupported_protocol(peer_id);
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
                let _res = self
                    .external_tx
                    .send(NetworkReceiverEvent::Message { source, data });
            }
            BehaviourEvent::Gossipsub(gossipsub::Event::GossipsubNotSupported { peer_id }) => {
                log::debug!("`gossipsub` protocol is not supported");
                self.swarm
                    .behaviour()
                    .peer_score
                    .handle()
                    .unsupported_protocol(peer_id);
            }
            BehaviourEvent::Gossipsub(_) => {}
            //
            BehaviourEvent::DbSync(db_sync::Event::ExternalValidation(validating_response)) => {
                let _res = self
                    .external_tx
                    .send(NetworkReceiverEvent::ExternalValidation(
                        validating_response,
                    ));
            }
            BehaviourEvent::DbSync(db_sync::Event::RequestSucceed {
                request_id: _,
                response,
            }) => {
                let _res = self
                    .external_tx
                    .send(NetworkReceiverEvent::DbResponse(Ok(response)));
            }
            BehaviourEvent::DbSync(db_sync::Event::RequestFailed {
                request_id: _,
                error,
            }) => {
                let _res = self
                    .external_tx
                    .send(NetworkReceiverEvent::DbResponse(Err(error)));
            }
            BehaviourEvent::DbSync(_) => {}
        }
    }

    fn handle_network_rx_event(&mut self, event: NetworkSenderEvent) {
        match event {
            NetworkSenderEvent::PublishMessage { data } => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gpu_commitments_topic(), data)
                {
                    log::debug!("gossipsub publishing failed: {e}")
                }
            }
            NetworkSenderEvent::RequestDbData(request) => {
                self.swarm.behaviour_mut().db_sync.request(request);
            }
            NetworkSenderEvent::RequestValidated(res) => {
                self.swarm.behaviour_mut().db_sync.request_validated(res);
            }
        }
    }
}

#[derive(NetworkBehaviour)]
pub(crate) struct Behaviour {
    // custom options to limit connections
    pub custom_connection_limits: custom_connection_limits::Behaviour,
    // limit connections
    pub connection_limits: connection_limits::Behaviour,
    // peer scoring system
    pub peer_score: peer_score::Behaviour,
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
    // database synchronization protocol
    pub db_sync: db_sync::Behaviour,
}

impl Behaviour {
    fn new(keypair: &identity::Keypair, db: Database) -> anyhow::Result<Self> {
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

        let peer_score = peer_score::Behaviour::new(peer_score::Config::default());
        let peer_score_handle = peer_score.handle();

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
            .map_err(|e| anyhow!("`gossipsub::ConfigBuilder::build()` error: {e}"))?;
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(keypair.clone()),
            gossip_config,
        )
        .map_err(|e| anyhow!("`gossipsub::Behaviour` error: {e}"))?;
        gossipsub
            .with_peer_score(
                gossipsub::PeerScoreParams::default(),
                gossipsub::PeerScoreThresholds::default(),
            )
            .map_err(|e| anyhow!("`gossipsub` scoring parameters error: {e}"))?;

        gossipsub.subscribe(&gpu_commitments_topic())?;

        let db_sync = db_sync::Behaviour::new(db_sync::Config::default(), peer_score_handle, db);

        Ok(Self {
            custom_connection_limits,
            connection_limits,
            peer_score,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::tests::init_logger;
    use ethexe_db::MemDb;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_memory_transport() {
        init_logger();

        let tmp_dir1 = tempfile::tempdir().unwrap();
        let config = NetworkEventLoopConfig::new_memory(tmp_dir1.path().to_path_buf(), "/memory/1");
        let signer1 = ethexe_signer::Signer::new(tmp_dir1.path().join("key")).unwrap();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let service1 = NetworkService::new(config.clone(), &signer1, db).unwrap();

        let peer_id = service1.event_loop.local_peer_id();
        let multiaddr: Multiaddr = format!("/memory/1/p2p/{peer_id}").parse().unwrap();

        let (sender, mut _service1_handle) =
            (service1.sender, tokio::spawn(service1.event_loop.run()));

        // second service
        let tmp_dir2 = tempfile::tempdir().unwrap();
        let signer2 = ethexe_signer::Signer::new(tmp_dir2.path().join("key")).unwrap();
        let mut config2 =
            NetworkEventLoopConfig::new_memory(tmp_dir2.path().to_path_buf(), "/memory/2");
        let db = Database::from_one(&MemDb::default(), [0; 20]);

        config2.bootstrap_addresses = [multiaddr].into();

        let service2 = NetworkService::new(config2.clone(), &signer2, db).unwrap();
        tokio::spawn(service2.event_loop.run());

        // Wait for the connection to be established
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Send a commitment from service1
        let commitment_data = b"test commitment".to_vec();

        sender.publish_message(commitment_data.clone());

        let mut receiver = service2.receiver;

        // Wait for the commitment to be received by service2
        let received_commitment = timeout(Duration::from_secs(5), async {
            while let Some(NetworkReceiverEvent::Message { source: _, data }) =
                receiver.recv().await
            {
                if data == commitment_data {
                    return Some(data);
                }
            }

            None
        })
        .await
        .expect("Timeout while waiting for commitment");

        assert!(received_commitment.is_some(), "Commitment was not received");
    }

    #[tokio::test]
    async fn request_db_data() {
        init_logger();

        let tmp_dir1 = tempfile::tempdir().unwrap();
        let config = NetworkEventLoopConfig::new_memory(tmp_dir1.path().to_path_buf(), "/memory/3");
        let signer1 = ethexe_signer::Signer::new(tmp_dir1.path().join("key")).unwrap();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let mut service1 = NetworkService::new(config.clone(), &signer1, db).unwrap();

        let peer_id = service1.event_loop.local_peer_id();
        let multiaddr: Multiaddr = format!("/memory/3/p2p/{peer_id}").parse().unwrap();

        tokio::spawn(service1.event_loop.run());

        // second service
        let tmp_dir2 = tempfile::tempdir().unwrap();
        let signer2 = ethexe_signer::Signer::new(tmp_dir2.path().join("key")).unwrap();
        let mut config2 =
            NetworkEventLoopConfig::new_memory(tmp_dir2.path().to_path_buf(), "/memory/4");
        let db = Database::from_one(&MemDb::default(), [0; 20]);

        config2.bootstrap_addresses = [multiaddr].into();

        let hello = db.write(b"hello");
        let world = db.write(b"world");

        let service2 = NetworkService::new(config2.clone(), &signer2, db).unwrap();
        tokio::spawn(service2.event_loop.run());

        // Wait for the connection to be established
        tokio::time::sleep(Duration::from_secs(1)).await;

        service1
            .sender
            .request_db_data(db_sync::Request::DataForHashes([hello, world].into()));

        let event = timeout(Duration::from_secs(5), service1.receiver.recv())
            .await
            .expect("time has elapsed")
            .unwrap();
        assert_eq!(
            event,
            NetworkReceiverEvent::DbResponse(Ok(db_sync::Response::DataForHashes(
                [(hello, b"hello".to_vec()), (world, b"world".to_vec())].into()
            )))
        );
    }

    #[tokio::test]
    async fn peer_blocked_by_score() {
        init_logger();

        let tmp_dir1 = tempfile::tempdir().unwrap();
        let config = NetworkEventLoopConfig::new_memory(tmp_dir1.path().to_path_buf(), "/memory/5");
        let signer1 = ethexe_signer::Signer::new(tmp_dir1.path().join("key")).unwrap();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let mut service1 = NetworkService::new(config.clone(), &signer1, db).unwrap();

        let peer_id = service1.event_loop.local_peer_id();
        let multiaddr: Multiaddr = format!("/memory/5/p2p/{peer_id}").parse().unwrap();

        let peer_score_handle = service1.event_loop.score_handle();

        tokio::spawn(service1.event_loop.run());

        // second service
        let tmp_dir2 = tempfile::tempdir().unwrap();
        let signer2 = ethexe_signer::Signer::new(tmp_dir2.path().join("key")).unwrap();
        let mut config2 =
            NetworkEventLoopConfig::new_memory(tmp_dir2.path().to_path_buf(), "/memory/6");
        config2.bootstrap_addresses = [multiaddr].into();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let service2 = NetworkService::new(config2.clone(), &signer2, db).unwrap();

        let service2_peer_id = service2.event_loop.local_peer_id();

        tokio::spawn(service2.event_loop.run());

        // Wait for the connection to be established
        tokio::time::sleep(Duration::from_secs(1)).await;

        peer_score_handle.unsupported_protocol(service2_peer_id);

        let event = service1.receiver.recv().await;
        assert_eq!(
            event,
            Some(NetworkReceiverEvent::PeerBlocked(service2_peer_id))
        );
    }

    #[tokio::test]
    async fn external_validation() {
        init_logger();

        let tmp_dir1 = tempfile::tempdir().unwrap();
        let config = NetworkEventLoopConfig::new_memory(tmp_dir1.path().to_path_buf(), "/memory/7");
        let signer1 = ethexe_signer::Signer::new(tmp_dir1.path().join("key")).unwrap();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let mut service1 = NetworkService::new(config.clone(), &signer1, db).unwrap();

        let peer_id = service1.event_loop.local_peer_id();
        let multiaddr: Multiaddr = format!("/memory/7/p2p/{peer_id}").parse().unwrap();

        tokio::spawn(service1.event_loop.run());

        // second service
        let tmp_dir2 = tempfile::tempdir().unwrap();
        let signer2 = ethexe_signer::Signer::new(tmp_dir2.path().join("key")).unwrap();
        let mut config2 =
            NetworkEventLoopConfig::new_memory(tmp_dir2.path().to_path_buf(), "/memory/8");
        config2.bootstrap_addresses = [multiaddr].into();
        let db = Database::from_one(&MemDb::default(), [0; 20]);
        let service2 = NetworkService::new(config2.clone(), &signer2, db).unwrap();
        tokio::spawn(service2.event_loop.run());

        // Wait for the connection to be established
        tokio::time::sleep(Duration::from_secs(1)).await;

        service1
            .sender
            .request_db_data(db_sync::Request::ProgramIds);

        let event = timeout(Duration::from_secs(5), service1.receiver.recv())
            .await
            .expect("time has elapsed")
            .unwrap();
        if let NetworkReceiverEvent::ExternalValidation(validating_response) = event {
            service1.sender.request_validated(Ok(validating_response));
        } else {
            unreachable!();
        }

        let event = timeout(Duration::from_secs(5), service1.receiver.recv())
            .await
            .expect("time has elapsed")
            .unwrap();
        assert_eq!(
            event,
            NetworkReceiverEvent::DbResponse(Ok(db_sync::Response::ProgramIds([].into())))
        );
    }
}
