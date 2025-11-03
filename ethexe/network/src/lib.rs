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
mod gossipsub;
mod injected;
mod kad;
pub mod peer_score;
mod utils;
mod validator;

pub mod export {
    pub use libp2p::{Multiaddr, PeerId, multiaddr::Protocol};
}

use crate::{db_sync::DbSyncDatabase, gossipsub::MessageAcceptance, validator::ValidatorDatabase};
use anyhow::{Context, anyhow};
use ethexe_common::{
    Address,
    ecdsa::PublicKey,
    injected::SignedInjectedTransaction,
    network::{SignedValidatorMessage, VerifiedValidatorMessage},
    tx_pool::SignedOffchainTransaction,
};
use ethexe_signer::Signer;
use futures::{Stream, future::Either, ready, stream::FusedStream};
use gprimitives::H256;
use libp2p::{
    Multiaddr, PeerId, Swarm, Transport, connection_limits,
    core::{muxing::StreamMuxerBox, transport, transport::ListenerId, upgrade},
    futures::StreamExt,
    identify, identity, mdns,
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
use std::{collections::HashSet, pin::Pin, task::Poll, time::Duration};
use validator::{list::ValidatorList, topic::ValidatorTopic};

pub const DEFAULT_LISTEN_PORT: u16 = 20333;

pub const PROTOCOL_VERSION: &str = "ethexe/0.1.0";
pub const AGENT_VERSION: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

const MAX_ESTABLISHED_INCOMING_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_OUTBOUND_PER_PEER_CONNECTIONS: u32 = 1;
const MAX_ESTABLISHED_INCOMING_CONNECTIONS: u32 = 100;

pub trait NetworkServiceDatabase: DbSyncDatabase + ValidatorDatabase {}
impl<T> NetworkServiceDatabase for T where T: DbSyncDatabase + ValidatorDatabase {}

#[derive(derive_more::Debug, Eq, PartialEq, Clone)]
pub enum NetworkEvent {
    ValidatorMessage(VerifiedValidatorMessage),
    OffchainTransaction(SignedOffchainTransaction),
    InjectedTransaction(SignedInjectedTransaction),
    PeerBlocked(PeerId),
    PeerConnected(PeerId),
}

#[derive(Default, Debug, Clone, Copy)]
pub enum TransportType {
    #[default]
    Default,
    Test,
}

impl TransportType {
    fn mdns_enabled(&self) -> bool {
        matches!(self, Self::Default)
    }
}

/// Config from CLI
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub public_key: PublicKey,
    pub router_address: Address,
    pub external_addresses: HashSet<Multiaddr>,
    pub bootstrap_addresses: HashSet<Multiaddr>,
    pub listen_addresses: HashSet<Multiaddr>,
    pub transport_type: TransportType,
}

impl NetworkConfig {
    pub fn new_local(public_key: PublicKey, router_address: Address) -> Self {
        Self {
            public_key,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: ["/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap()].into(),
            transport_type: TransportType::Default,
            router_address,
        }
    }

    pub fn new_test(public_key: PublicKey, router_address: Address) -> Self {
        Self {
            public_key,
            external_addresses: Default::default(),
            bootstrap_addresses: Default::default(),
            listen_addresses: Default::default(),
            transport_type: TransportType::Test,
            router_address,
        }
    }
}

/// Config from other services
pub struct NetworkRuntimeConfig {
    pub genesis_block_hash: H256,
    pub validator_key: Option<PublicKey>,
    pub general_signer: Signer,
    pub network_signer: Signer,
    pub external_data_provider: Box<dyn db_sync::ExternalDataProvider>,
    pub db: Box<dyn NetworkServiceDatabase>,
}

pub struct NetworkService {
    swarm: Swarm<Behaviour>,
    // `MemoryTransport` doesn't unregister its ports on drop so we do it
    listeners: Vec<ListenerId>,
    validator_list: ValidatorList,
    validator_topic: ValidatorTopic,
}

impl Stream for NetworkService {
    type Item = NetworkEvent;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        if let Some(message) = self.validator_topic.next_message() {
            return Poll::Ready(Some(NetworkEvent::ValidatorMessage(message)));
        }

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
        runtime_config: NetworkRuntimeConfig,
    ) -> anyhow::Result<NetworkService> {
        let NetworkConfig {
            public_key,
            external_addresses,
            bootstrap_addresses,
            listen_addresses,
            transport_type,
            router_address,
        } = config;

        let NetworkRuntimeConfig {
            genesis_block_hash,
            validator_key,
            general_signer,
            network_signer,
            external_data_provider,
            db,
        } = runtime_config;

        let keypair = NetworkService::generate_keypair(&network_signer, public_key)?;

        let behaviour_config = BehaviourConfig {
            router_address,
            keypair: keypair.clone(),
            external_data_provider,
            db: DbSyncDatabase::clone_boxed(&db),
            enable_mdns: transport_type.mdns_enabled(),
            validator_key,
            general_signer,
        };
        let mut swarm =
            NetworkService::create_swarm(keypair.clone(), transport_type, behaviour_config)?;

        for multiaddr in external_addresses {
            swarm.add_external_address(multiaddr);
        }

        let mut listeners = Vec::new();
        for multiaddr in listen_addresses {
            let id = swarm.listen_on(multiaddr).context("`listen_on()` failed")?;
            listeners.push(id);
        }

        for multiaddr in bootstrap_addresses {
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

            swarm.behaviour_mut().kad.add_address(peer_id, multiaddr);
        }

        let validator_list =
            ValidatorList::new(genesis_block_hash, ValidatorDatabase::clone_boxed(&db))
                .context("failed to create validator list")?;

        let validator_topic = ValidatorTopic::new(
            ValidatorDatabase::clone_boxed(&db),
            swarm.behaviour().peer_score.handle(),
        );

        let mut this = Self {
            swarm,
            listeners,
            validator_list,
            validator_topic,
        };

        // bootstrap validator structures
        this.on_new_era();

        Ok(this)
    }

    fn generate_keypair(signer: &Signer, key: PublicKey) -> anyhow::Result<identity::Keypair> {
        let key = signer.storage().get_private_key(key)?;
        let key = identity::secp256k1::SecretKey::try_from_bytes(&mut <[u8; 32]>::from(key))
            .expect("Signer provided invalid key; qed");
        let pair = identity::secp256k1::Keypair::from(key);
        Ok(identity::Keypair::from(pair))
    }

    fn create_transport(
        keypair: &identity::Keypair,
        transport_type: TransportType,
    ) -> anyhow::Result<transport::Boxed<(PeerId, StreamMuxerBox)>> {
        match transport_type {
            TransportType::Default => {
                let tcp = libp2p::tcp::tokio::Transport::default()
                    .upgrade(upgrade::Version::V1Lazy)
                    .authenticate(libp2p::tls::Config::new(keypair)?)
                    .multiplex(yamux::Config::default())
                    .timeout(Duration::from_secs(20));

                let quic_config = libp2p::quic::Config::new(keypair);
                let quic = libp2p::quic::tokio::Transport::new(quic_config);

                Ok(quic
                    .or_transport(tcp)
                    .map(|either_output, _| match either_output {
                        Either::Left((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
                        Either::Right((peer_id, muxer)) => (peer_id, StreamMuxerBox::new(muxer)),
                    })
                    .boxed())
            }
            TransportType::Test => Ok(transport::MemoryTransport::default()
                .or_transport(libp2p::tcp::tokio::Transport::default())
                .upgrade(upgrade::Version::V1Lazy)
                .authenticate(libp2p::plaintext::Config::new(keypair))
                .multiplex(yamux::Config::default())
                .timeout(Duration::from_secs(20))
                .boxed()),
        }
    }

    fn create_swarm(
        keypair: identity::Keypair,
        transport_type: TransportType,
        config: BehaviourConfig,
    ) -> anyhow::Result<Swarm<Behaviour>> {
        let transport = Self::create_transport(&keypair, transport_type)?;

        let behaviour = Behaviour::new(config)?;

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
            BehaviourEvent::CustomConnectionLimits(event) => match event {},
            BehaviourEvent::ConnectionLimits(event) => match event {},
            BehaviourEvent::PeerScore(event) => return self.handle_peer_score_event(event),
            BehaviourEvent::Ping(event) => self.handle_ping_event(event),
            BehaviourEvent::Identify(event) => self.handle_identify_event(event),
            BehaviourEvent::Mdns4(event) => self.handle_mdns_event(event),
            BehaviourEvent::Kad(event) => self.handle_kad_event(event),
            BehaviourEvent::Gossipsub(event) => return self.handle_gossipsub_event(event),
            BehaviourEvent::DbSync(_event) => {}
            BehaviourEvent::Injected(event) => return self.handle_injected_event(event),
            BehaviourEvent::ValidatorDiscovery(event) => {
                self.handle_validator_discovery_event(event)
            }
        }

        None
    }

    fn handle_peer_score_event(&mut self, event: peer_score::Event) -> Option<NetworkEvent> {
        match event {
            peer_score::Event::PeerBlocked {
                peer_id,
                last_reason: _,
            } => Some(NetworkEvent::PeerBlocked(peer_id)),
            _ => None,
        }
    }

    fn handle_ping_event(&mut self, event: ping::Event) {
        let ping::Event {
            peer,
            connection: _,
            result,
        } = event;

        if let Err(e) = result {
            log::debug!("ping to {peer} failed: {e}. Disconnecting...");
            let _res = self.swarm.disconnect_peer_id(peer);
        }
    }

    fn handle_identify_event(&mut self, event: identify::Event) {
        match event {
            identify::Event::Received { peer_id, info, .. } => {
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
                    behaviour.kad.add_address(peer_id, listen_addr);
                }
            }
            identify::Event::Error { peer_id, error, .. } => {
                log::debug!("{peer_id} is not identified: {error}");
                self.swarm
                    .behaviour()
                    .peer_score
                    .handle()
                    .unsupported_protocol(peer_id);
            }
            _ => {}
        }
    }

    fn handle_mdns_event(&mut self, event: mdns::Event) {
        match event {
            mdns::Event::Discovered(peers) => {
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
            mdns::Event::Expired(peers) => {
                for (peer_id, _multiaddr) in peers {
                    let _res = self.swarm.disconnect_peer_id(peer_id);
                }
            }
        }
    }

    fn handle_kad_event(&mut self, event: kad::Event) {
        match event {
            kad::Event::RoutingUpdated { peer } => {
                let behaviour = self.swarm.behaviour_mut();
                if let Some(mdns4) = behaviour.mdns4.as_ref()
                    && mdns4.discovered_nodes().any(|&p| p == peer)
                {
                    // we don't want local peers to appear in KadDHT.
                    // event can be emitted few times in a row for
                    // the same peer, so we just ignore `None`
                    let _res = behaviour.kad.remove_peer(peer);
                }
            }
            kad::Event::InboundPutRecord { source, validator } => {
                let behaviour = self.swarm.behaviour_mut();
                let res = validator.validate(&mut behaviour.kad, |record| {
                    behaviour
                        .validator_discovery
                        .put_identity(&self.validator_list, record)
                });

                if let Err(err) = res {
                    log::trace!("failed to put identity: {err}");
                    behaviour.peer_score.handle().invalid_data(source);
                }
            }
        }
    }

    fn handle_gossipsub_event(&mut self, event: gossipsub::Event) -> Option<NetworkEvent> {
        match event {
            gossipsub::Event::Message { source, validator } => {
                let gossipsub = &mut self.swarm.behaviour_mut().gossipsub;

                validator.validate(gossipsub, |message| match message {
                    gossipsub::Message::Commitments(message) => {
                        let (acceptance, message) = self.validator_topic.verify_message_initially(
                            &self.validator_list,
                            source,
                            message,
                        );
                        (acceptance, message.map(NetworkEvent::ValidatorMessage))
                    }
                    gossipsub::Message::Offchain(transaction) => (
                        MessageAcceptance::Accept,
                        Some(NetworkEvent::OffchainTransaction(transaction)),
                    ),
                })
            }
            gossipsub::Event::PublishFailure {
                error,
                message,
                topic,
            } => {
                log::warn!(
                    "failed to publish gossip `{message:?}` message to {topic} topic: {error}"
                );
                None
            }
        }
    }

    fn handle_injected_event(&mut self, event: injected::Event) -> Option<NetworkEvent> {
        match event {
            injected::Event::NewInjectedTransaction(transaction) => {
                Some(NetworkEvent::InjectedTransaction(transaction))
            }
        }
    }

    fn handle_validator_discovery_event(&mut self, event: validator::discovery::Event) {
        match event {
            validator::discovery::Event::QueryIdentities => {
                let behaviour = self.swarm.behaviour_mut();
                for key in behaviour
                    .validator_discovery
                    .identity_keys(&self.validator_list)
                {
                    behaviour.kad.get_record(key);
                }
            }
            validator::discovery::Event::PutIdentity => {
                let behaviour = self.swarm.behaviour_mut();

                match behaviour.validator_discovery.identity(&self.validator_list) {
                    Some(Ok(identity)) => {
                        if let Err(err) = behaviour.kad.put_record(identity) {
                            log::warn!("failed to put record into local storage: {err}");
                        }
                    }
                    Some(Err(err)) => {
                        log::warn!("failed to create validator identity: {err}");
                    }
                    None => {
                        // validator public key is not set
                    }
                }
            }
        }
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    pub fn score_handle(&self) -> peer_score::Handle {
        self.swarm.behaviour().peer_score.handle()
    }

    pub fn db_sync_handle(&self) -> db_sync::Handle {
        self.swarm.behaviour().db_sync.handle()
    }

    fn on_new_era(&mut self) {
        // revalidate cached messages
        self.validator_topic.on_new_era(&self.validator_list);
    }

    pub fn set_chain_head(&mut self, chain_head: H256) -> anyhow::Result<()> {
        let era_changed = self.validator_list.set_chain_head(chain_head)?;

        if era_changed {
            self.on_new_era();
        }

        Ok(())
    }

    pub fn publish_message(&mut self, data: impl Into<SignedValidatorMessage>) {
        self.swarm.behaviour_mut().gossipsub.publish(data.into())
    }

    pub fn publish_offchain_transaction(&mut self, data: SignedOffchainTransaction) {
        self.swarm.behaviour_mut().gossipsub.publish(data);
    }

    pub fn send_injected_transaction(&mut self, data: SignedInjectedTransaction) {
        let behaviour = self.swarm.behaviour_mut();
        behaviour
            .injected
            .send_transaction(&mut behaviour.validator_discovery, data);
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

struct BehaviourConfig {
    router_address: Address,
    keypair: identity::Keypair,
    external_data_provider: Box<dyn db_sync::ExternalDataProvider>,
    db: Box<dyn DbSyncDatabase>,
    enable_mdns: bool,
    validator_key: Option<PublicKey>,
    general_signer: Signer,
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
    pub kad: kad::Behaviour,
    // general communication
    pub gossipsub: gossipsub::Behaviour,
    // database synchronization protocol
    pub db_sync: db_sync::Behaviour,
    // injected transaction shenanigans
    pub injected: injected::Behaviour,
    // validator discovery
    pub validator_discovery: validator::discovery::Behaviour,
}

impl Behaviour {
    fn new(config: BehaviourConfig) -> anyhow::Result<Self> {
        let BehaviourConfig {
            router_address,
            keypair,
            external_data_provider,
            db,
            enable_mdns,
            validator_key,
            general_signer,
        } = config;

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

        let kad = kad::Behaviour::new(peer_id);

        let gossipsub =
            gossipsub::Behaviour::new(keypair.clone(), peer_score_handle.clone(), router_address)
                .map_err(|e| anyhow!("`gossipsub::Behaviour` error: {e}"))?;

        let db_sync = db_sync::Behaviour::new(
            db_sync::Config::default(),
            peer_score_handle.clone(),
            external_data_provider,
            db,
        );

        let injected = injected::Behaviour::new(peer_score_handle);

        let validator_discovery =
            validator::discovery::Behaviour::new(keypair, validator_key, general_signer);

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
            injected,
            validator_discovery,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db_sync::{ExternalDataProvider, tests::fill_data_provider},
        utils::tests::init_logger,
    };
    use async_trait::async_trait;
    use ethexe_common::{BlockHeader, ProtocolTimelines, db::OnChainStorageRW, gear::CodeState};
    use ethexe_db::{Database, MemDb};
    use ethexe_signer::{FSKeyStorage, Signer};
    use gprimitives::{ActorId, CodeId, H256};
    use nonempty::nonempty;
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
        const GENESIS_BLOCK: H256 = H256::zero();
        const TIMELINES: ProtocolTimelines = ProtocolTimelines {
            genesis_ts: 1_000_000,
            era: 1,
            election: 1,
        };

        db.set_block_header(
            GENESIS_BLOCK,
            BlockHeader {
                height: 0,
                timestamp: 0,
                parent_hash: Default::default(),
            },
        );
        db.set_validators(0, nonempty![Address::default()].into());
        db.set_protocol_timelines(TIMELINES);

        let key_storage = FSKeyStorage::tmp();
        let signer = Signer::new(key_storage);
        let key = signer.generate_key().unwrap();
        let config = NetworkConfig::new_test(key, Address::default());

        let runtime_config = NetworkRuntimeConfig {
            genesis_block_hash: GENESIS_BLOCK,
            validator_key: None,
            general_signer: signer.clone(),
            network_signer: signer,
            external_data_provider: Box::new(data_provider),
            db: Box::new(db),
        };

        NetworkService::new(config.clone(), runtime_config).unwrap()
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
