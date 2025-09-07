// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
    pub use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};
}

use anyhow::{Context, anyhow};
use ethexe_common::ecdsa::PublicKey;
use ethexe_db::Database;
use ethexe_signer::Signer;
use futures::{Stream, future::Either, ready, stream::FusedStream};
use gprimitives::utils::ByteSliceFormatter;
use libp2p::{
    Multiaddr, PeerId, Swarm, Transport, connection_limits,
    core::{muxing::StreamMuxerBox, transport::ListenerId, upgrade},
    futures::StreamExt,
    gossipsub, identify, identity, kad, mdns,
    multiaddr::Protocol,
    ping,
    swarm::{
        Config as SwarmConfig, NetworkBehaviour, SwarmEvent,
        behaviour::toggle::Toggle,
        dial_opts::{DialOpts, PeerCondition},
    },
    yamux,
};
#[cfg(test)]
use libp2p_swarm_test::SwarmExt;
use std::{
    collections::HashSet,
    fmt, fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    pin::Pin,
    str::FromStr,
    task::Poll,
    time::Duration,
};

pub const DEFAULT_LISTEN_PORT: u16 = 20333;

pub const PROTOCOL_VERSION: &str = "ethexe/0.1.0";
pub const AGENT_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

const MAX_ESTABLISHED_INCOMING_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_OUTBOUND_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_INCOMING_CONNECTIONS: u32 = 100;

#[derive(Eq, PartialEq)]
pub enum NetworkEvent {
    Message {
        data: Vec<u8>,
        source: Option<PeerId>,
    },
    PeerBlocked(PeerId),
    PeerConnected(PeerId),
}

impl fmt::Debug for NetworkEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkEvent::Message { data, source } => f
                .debug_struct("Message")
                .field(
                    "data",
                    &format_args!(
                        "{:.8} ({} bytes)",
                        ByteSliceFormatter::Dynamic(data),
                        data.len()
                    ),
                )
                .field("source", source)
                .finish(),
            NetworkEvent::PeerBlocked(peer_id) => {
                f.debug_tuple("PeerBlocked").field(peer_id).finish()
            }
            NetworkEvent::PeerConnected(peer_id) => {
                f.debug_tuple("PeerConnected").field(peer_id).finish()
            }
        }
    }
}

#[derive(Default, Debug, Clone)]
pub enum TransportType {
    #[default]
    Default,
    Test,
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub config_dir: PathBuf,
    pub public_key: Option<PublicKey>,
    pub external_addresses: HashSet<Multiaddr>,
    pub bootstrap_addresses: HashSet<Multiaddr>,
    pub listen_addresses: HashSet<Multiaddr>,
    pub transport_type: TransportType,
}

impl NetworkConfig {
    pub fn new_local(config_path: PathBuf) -> Self {
        Self {
            config_dir: config_path,
            public_key: None,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: ["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()].into(),
            transport_type: TransportType::Default,
        }
    }

    pub fn new_test(config_path: PathBuf) -> Self {
        Self {
            config_dir: config_path,
            public_key: None,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: Default::default(),
            transport_type: TransportType::Test,
        }
    }
}

pub struct NetworkService {
    swarm: Swarm<Behaviour>,
    // `MemoryTransport` doesn't unregister its ports on drop so we do it
    listeners: Vec<ListenerId>,
}

impl Stream for NetworkService {
    type Item = NetworkEvent;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            let Some(event) = ready!(self.swarm.poll_next_unpin(cx)) else {
                return Poll::Ready(None);
            };

            if let Some(event) = self.handle_swarm_event(event) {
                return Poll::Ready(Some(event));
            }
        }
    }
}

impl FusedStream for NetworkService {
    fn is_terminated(&self) -> bool {
        self.swarm.is_terminated()
    }
}

impl NetworkService {
    pub fn new(
        config: NetworkConfig,
        signer: &Signer,
        external_data_provider: Box<dyn db_sync::ExternalDataProvider>,
        db: Database,
    ) -> anyhow::Result<NetworkService> {
        fs::create_dir_all(&config.config_dir)
            .context("failed to create network configuration directory")?;

        let keypair =
            NetworkService::generate_keypair(signer, &config.config_dir, config.public_key)?;
        let mut swarm = NetworkService::create_swarm(
            keypair,
            external_data_provider,
            db,
            config.transport_type,
        )?;

        for multiaddr in config.external_addresses {
            swarm.add_external_address(multiaddr);
        }

        let mut listeners = Vec::new();
        for multiaddr in config.listen_addresses {
            let id = swarm.listen_on(multiaddr).context("`listen_on()` failed")?;
            listeners.push(id);
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

        Ok(Self { swarm, listeners })
    }

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

        let key = signer.storage().get_private_key(key)?;
        let key = identity::secp256k1::SecretKey::try_from_bytes(&mut <[u8; 32]>::from(key))
            .expect("Signer provided invalid key; qed");
        let pair = identity::secp256k1::Keypair::from(key);
        Ok(identity::Keypair::from(pair))
    }

    fn create_swarm(
        keypair: identity::Keypair,
        external_data_provider: Box<dyn db_sync::ExternalDataProvider>,
        db: Database,
        transport_type: TransportType,
    ) -> anyhow::Result<Swarm<Behaviour>> {
        let transport = match transport_type {
            TransportType::Default => {
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
            TransportType::Test => libp2p::core::transport::MemoryTransport::default()
                .or_transport(libp2p::tcp::tokio::Transport::default())
                .upgrade(upgrade::Version::V1Lazy)
                .authenticate(libp2p::plaintext::Config::new(&keypair))
                .multiplex(yamux::Config::default())
                .timeout(Duration::from_secs(20))
                .boxed(),
        };

        let enable_mdns = match transport_type {
            TransportType::Default => true,
            TransportType::Test => false,
        };

        let behaviour = Behaviour::new(&keypair, external_data_provider, db, enable_mdns)?;
        let local_peer_id = keypair.public().to_peer_id();
        let mut config = SwarmConfig::with_tokio_executor();

        if let TransportType::Test = transport_type {
            config = config.with_idle_connection_timeout(Duration::from_secs(5));
        }

        Ok(Swarm::new(transport, behaviour, local_peer_id, config))
    }

    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourEvent>) -> Option<NetworkEvent> {
        log::trace!("new swarm event: {event:?}");

        match event {
            SwarmEvent::Behaviour(e) => self.handle_behaviour_event(e),
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                Some(NetworkEvent::PeerConnected(peer_id))
            }
            _ => None,
        }
    }

    fn handle_behaviour_event(&mut self, event: BehaviourEvent) -> Option<NetworkEvent> {
        match event {
            BehaviourEvent::CustomConnectionLimits(infallible) => match infallible {},
            //
            BehaviourEvent::ConnectionLimits(infallible) => match infallible {},
            //
            BehaviourEvent::PeerScore(peer_score::Event::PeerBlocked {
                peer_id,
                last_reason: _,
            }) => return Some(NetworkEvent::PeerBlocked(peer_id)),
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
                if let Some(mdns4) = behaviour.mdns4.as_ref()
                    && mdns4.discovered_nodes().any(|&p| p == peer)
                {
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
            }) if commitments_topic().hash() == topic || offchain_tx_topic().hash() == topic => {
                return Some(NetworkEvent::Message { source, data });
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
            BehaviourEvent::DbSync(_) => {}
        }

        None
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    pub fn score_handle(&self) -> peer_score::Handle {
        self.swarm.behaviour().peer_score.handle()
    }

    pub fn db_sync_handle(&self) -> db_sync::DbSyncHandle {
        self.swarm.behaviour().db_sync.handle()
    }

    pub fn publish_message(&mut self, data: Vec<u8>) {
        if let Err(e) = self
            .swarm
            .behaviour_mut()
            .gossipsub
            .publish(commitments_topic(), data)
        {
            log::error!("gossipsub publishing failed: {e}")
        }
    }

    pub fn publish_offchain_transaction(&mut self, data: Vec<u8>) {
        if let Err(e) = self
            .swarm
            .behaviour_mut()
            .gossipsub
            .publish(offchain_tx_topic(), data)
        {
            log::error!("gossipsub publishing failed: {e}")
        }
    }
}

impl Drop for NetworkService {
    fn drop(&mut self) {
        for id in self.listeners.drain(..) {
            self.swarm.remove_listener(id);
        }
    }
}

#[cfg(test)]
impl NetworkService {
    async fn connect(&mut self, service: &mut Self) {
        self.swarm.listen().with_memory_addr_external().await;
        service.swarm.listen().with_memory_addr_external().await;
        self.swarm.connect(&mut service.swarm).await;
    }

    async fn loop_on_next(mut self) {
        while let Some(event) = self.next().await {
            log::trace!("loop_on_next: {event:?}");
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
    pub mdns4: Toggle<mdns::tokio::Behaviour>,
    // global traversal discovery
    // TODO: consider to cache records in fs
    pub kad: kad::Behaviour<kad::store::MemoryStore>,
    // general communication
    pub gossipsub: gossipsub::Behaviour,
    // database synchronization protocol
    pub db_sync: db_sync::Behaviour,
}

impl Behaviour {
    fn new(
        keypair: &identity::Keypair,
        external_data_provider: Box<dyn db_sync::ExternalDataProvider>,
        db: Database,
        enable_mdns: bool,
    ) -> anyhow::Result<Self> {
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

        let mdns4 = Toggle::from(
            enable_mdns
                .then(|| mdns::Behaviour::new(mdns::Config::default(), peer_id))
                .transpose()?,
        );

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

        gossipsub.subscribe(&commitments_topic())?;
        gossipsub.subscribe(&offchain_tx_topic())?;

        let db_sync = db_sync::Behaviour::new(
            db_sync::Config::default(),
            peer_score_handle,
            external_data_provider,
            db,
        );

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

fn commitments_topic() -> gossipsub::IdentTopic {
    // TODO: use router address in topic name to avoid obsolete router
    gossipsub::IdentTopic::new("ethexe-commitments")
}

fn offchain_tx_topic() -> gossipsub::IdentTopic {
    gossipsub::IdentTopic::new("ethexe-tx-pool")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db_sync::{ExternalDataProvider, tests::fill_data_provider},
        utils::tests::init_logger,
    };
    use async_trait::async_trait;
    use ethexe_common::gear::CodeState;
    use ethexe_db::MemDb;
    use ethexe_signer::{FSKeyStorage, Signer};
    use gprimitives::{ActorId, CodeId, H256};
    use std::{
        collections::{BTreeSet, HashMap},
        sync::Arc,
    };
    use tokio::{
        sync::RwLock,
        time::{Duration, timeout},
    };

    #[derive(Default)]
    struct DataProviderInner {
        programs_code_ids_at: HashMap<(BTreeSet<ActorId>, H256), Vec<CodeId>>,
        code_states_at: HashMap<(BTreeSet<CodeId>, H256), Vec<CodeState>>,
    }

    #[derive(Default, Clone)]
    pub struct DataProvider(Arc<RwLock<DataProviderInner>>);

    impl DataProvider {
        pub async fn set_programs_code_ids_at(
            &self,
            program_ids: BTreeSet<ActorId>,
            at: H256,
            code_ids: Vec<CodeId>,
        ) {
            self.0
                .write()
                .await
                .programs_code_ids_at
                .insert((program_ids, at), code_ids);
        }
    }

    #[async_trait]
    impl ExternalDataProvider for DataProvider {
        fn clone_boxed(&self) -> Box<dyn ExternalDataProvider> {
            Box::new(self.clone())
        }

        async fn programs_code_ids_at(
            self: Box<Self>,
            program_ids: BTreeSet<ActorId>,
            block: H256,
        ) -> anyhow::Result<Vec<CodeId>> {
            assert!(!program_ids.is_empty());
            Ok(self
                .0
                .read()
                .await
                .programs_code_ids_at
                .get(&(program_ids, block))
                .cloned()
                .unwrap_or_default())
        }

        async fn codes_states_at(
            self: Box<Self>,
            code_ids: BTreeSet<CodeId>,
            block: H256,
        ) -> anyhow::Result<Vec<CodeState>> {
            assert!(!code_ids.is_empty());
            Ok(self
                .0
                .read()
                .await
                .code_states_at
                .get(&(code_ids, block))
                .cloned()
                .unwrap_or_default())
        }
    }

    fn new_service_with(db: Database, data_provider: DataProvider) -> NetworkService {
        let key_storage = FSKeyStorage::tmp();
        let config = NetworkConfig::new_test(key_storage.path.clone().join("network"));
        let signer = Signer::new(key_storage);
        NetworkService::new(config.clone(), &signer, Box::new(data_provider), db).unwrap()
    }

    fn new_service() -> NetworkService {
        new_service_with(Database::memory(), DataProvider::default())
    }

    #[tokio::test]
    async fn test_memory_transport() {
        init_logger();

        let mut service1 = new_service();
        let mut service2 = new_service();

        service1.connect(&mut service2).await;
    }

    #[tokio::test]
    async fn request_db_data() {
        init_logger();

        let mut service1 = new_service();
        let service1_handle = service1.db_sync_handle();

        // second service
        let db = Database::from_one(&MemDb::default());

        let hello = db.write_hash(b"hello");
        let world = db.write_hash(b"world");

        let mut service2 = new_service_with(db, Default::default());

        service1.connect(&mut service2).await;
        tokio::spawn(service1.loop_on_next());
        tokio::spawn(service2.loop_on_next());

        let request = service1_handle.request(db_sync::Request::hashes([hello, world]));
        let response = timeout(Duration::from_secs(5), request)
            .await
            .expect("time has elapsed")
            .unwrap();
        assert_eq!(
            response,
            db_sync::Response::Hashes(
                [(hello, b"hello".to_vec()), (world, b"world".to_vec())].into()
            )
        );
    }

    #[tokio::test]
    async fn peer_blocked_by_score() {
        init_logger();

        let mut service1 = new_service();
        let peer_score_handle = service1.score_handle();

        // second service
        let mut service2 = new_service();
        let service2_peer_id = service2.local_peer_id();

        service1.connect(&mut service2).await;
        tokio::spawn(service2.loop_on_next());

        peer_score_handle.unsupported_protocol(service2_peer_id);

        let event = timeout(Duration::from_secs(5), service1.next())
            .await
            .expect("time has elapsed")
            .unwrap();
        assert_eq!(event, NetworkEvent::PeerBlocked(service2_peer_id));
    }

    #[tokio::test]
    async fn external_data_provider() {
        init_logger();

        let alice_data_provider = DataProvider::default();
        let mut alice = new_service_with(Database::memory(), alice_data_provider.clone());
        let alice_handle = alice.db_sync_handle();
        let bob_db = Database::memory();
        let mut bob = new_service_with(bob_db.clone(), DataProvider::default());

        alice.connect(&mut bob).await;
        tokio::spawn(alice.loop_on_next());
        tokio::spawn(bob.loop_on_next());

        let expected_response = fill_data_provider(alice_data_provider, bob_db).await;

        let request = alice_handle.request(db_sync::Request::program_ids(H256::zero(), 2));
        let response = timeout(Duration::from_secs(5), request)
            .await
            .expect("time has elapsed")
            .unwrap();
        assert_eq!(response, expected_response);
    }
}
