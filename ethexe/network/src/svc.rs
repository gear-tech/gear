#![allow(unused)]

pub mod export {
    pub use libp2p::{multiaddr::Protocol, Multiaddr, PeerId};
}

use crate::{db_sync, BehaviourEvent, AGENT_VERSION, PROTOCOL_VERSION};
use anyhow::{anyhow, Context, Result};
use ethexe_common::gear::{BlockCommitment, CodeCommitment};
use ethexe_db::Database;
use ethexe_sequencer::agro::AggregatedCommitments;
use ethexe_signer::{Digest, PublicKey, Signature, Signer};
use ethexe_validator::BlockCommitmentValidationRequest;
use futures::future::Either;
use libp2p::{
    connection_limits,
    core::{muxing::StreamMuxerBox, upgrade},
    futures::StreamExt,
    gossipsub, identify,
    identity::{self, secp256k1},
    kad, mdns,
    multiaddr::Protocol,
    ping,
    swarm::{
        dial_opts::{DialOpts, PeerCondition},
        Config as SwarmConfig, NetworkBehaviour, SwarmEvent,
    },
    yamux, Multiaddr, PeerId, Swarm, Transport,
};
use parity_scale_codec::{Decode, Encode};
use std::{
    collections::HashSet,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use tokio::{select, sync::mpsc, task::JoinHandle};

use crate::{gpu_commitments_topic, peer_score, Behaviour, TransportType};

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

    pub fn get_or_generate_keypair(&self, signer: &Signer) -> Result<identity::Keypair> {
        let key = if let Some(key) = self.public_key {
            log::trace!("use networking key from command-line arguments");
            key
        } else {
            let public_key_path = self.config_dir.join("public_key");
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

        let mut private_key = signer.get_private_key(key)?;

        let private_key = secp256k1::SecretKey::try_from_bytes(&mut private_key.0)
            .expect("Signer provided invalid key; qed");

        let pair = secp256k1::Keypair::from(private_key);

        Ok(pair.into())
    }

    fn create_swarm(&self, keypair: identity::Keypair, db: Database) -> Result<Swarm<Behaviour>> {
        let transport = match self.transport_type {
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

        let mut swarm = Swarm::new(transport, behaviour, local_peer_id, config);

        for multiaddr in self.external_addresses.clone() {
            swarm.add_external_address(multiaddr);
        }

        for multiaddr in self.listen_addresses.clone() {
            swarm.listen_on(multiaddr).context("`listen_on()` failed")?;
        }

        for multiaddr in self.bootstrap_addresses.clone() {
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

        Ok(swarm)
    }
}

pub struct Service {
    swarm: Swarm<Behaviour>,
}

impl Service {
    pub fn new(config: &NetworkConfig, signer: &Signer, db: Database) -> Result<Self> {
        fs::create_dir_all(&config.config_dir)
            .context("failed to create network configuration directory")?;

        let keypair = config.get_or_generate_keypair(signer)?;

        let swarm = config.create_swarm(keypair, db)?;

        Ok(Self { swarm })
    }

    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    pub fn score_handle(&self) -> peer_score::Handle {
        self.swarm.behaviour().peer_score.handle()
    }

    pub fn run(mut self) -> (JoinHandle<Result<()>>, RequestSender, EventReceiver) {
        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (mut event_tx, event_rx) = mpsc::unbounded_channel();

        let handle = tokio::spawn(async move {
            loop {
                select! {
                    event = self.swarm.select_next_some() => self.handle_swarm_event(event, &mut event_tx),
                    request = request_rx.recv() => match request {
                        Some(request) => {
                            self.handle_request(request);
                        }
                        None => {
                            log::info!("Network channel has been disconnected, shutting down network service...");
                            break;
                        },
                    },
                }
            }

            Ok(())
        });

        (handle, RequestSender(request_tx), EventReceiver(event_rx))
    }

    fn handle_swarm_event(
        &mut self,
        event: SwarmEvent<BehaviourEvent>,
        event_tx: &mut mpsc::UnboundedSender<Event>,
    ) {
        log::trace!("new swarm event: {event:?}");

        #[allow(clippy::single_match)]
        match event {
            SwarmEvent::Behaviour(e) => self.handle_behaviour_event(e, event_tx),
            _ => {}
        }
    }

    fn handle_behaviour_event(
        &mut self,
        event: BehaviourEvent,
        event_tx: &mut mpsc::UnboundedSender<Event>,
    ) {
        match event {
            BehaviourEvent::CustomConnectionLimits(void) => void::unreachable(void),
            //
            BehaviourEvent::ConnectionLimits(void) => void::unreachable(void),
            //
            BehaviourEvent::PeerScore(peer_score::Event::PeerBlocked {
                peer_id,
                last_reason: _,
            }) => {
                // TODO: send event
                // let _res = event_tx.send(NetworkReceiverEvent::PeerBlocked(peer_id));
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
                if let Ok(message) = Message::decode(&mut &data[..]) {
                    let _res = event_tx.send(Event::MessageReceived {
                        source,
                        message: Box::new(message),
                    });
                } else {
                    log::trace!("failed to decode message from {source:?}");
                }
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
                // TODO: send event
                // let _res = event_tx.send(NetworkReceiverEvent::ExternalValidation(
                //     validating_response,
                // ));
            }
            BehaviourEvent::DbSync(db_sync::Event::RequestSucceed {
                request_id: _,
                response,
            }) => {
                // TODO: send event
                // let _res = event_tx.send(NetworkReceiverEvent::DbResponse(Ok(response)));
            }
            BehaviourEvent::DbSync(db_sync::Event::RequestFailed {
                request_id: _,
                error,
            }) => {
                // TODO: send event
                // let _res = event_tx.send(NetworkReceiverEvent::DbResponse(Err(error)));
            }
            BehaviourEvent::DbSync(_) => {}
        }
    }

    fn handle_request(&mut self, request: Request) {
        match request {
            Request::PublishMessage { data } => {
                if let Err(e) = self
                    .swarm
                    .behaviour_mut()
                    .gossipsub
                    .publish(gpu_commitments_topic(), data.encode())
                {
                    log::debug!("gossipsub publishing failed: {e}");
                }
            }
            Request::Sync() => {
                // self.swarm.behaviour_mut().db_sync.request(request);
                // self.swarm.behaviour_mut().db_sync.request_validated(res);
            }
        }
    }
}

// TODO: make message type generic
pub enum Event {
    MessageReceived {
        source: Option<PeerId>,
        message: Box<Message>,
    },
    Sync(/* db_sync::Event */),
}

pub struct EventReceiver(mpsc::UnboundedReceiver<Event>);

impl EventReceiver {
    pub async fn recv(&mut self) -> Result<Event> {
        self.0
            .recv()
            .await
            .ok_or_else(|| anyhow!("connection closed"))
    }
}

pub enum Request {
    PublishMessage { data: Message },
    Sync(/* dn_sync::Request */),
}

#[derive(Clone)]
pub struct RequestSender(mpsc::UnboundedSender<Request>);

impl RequestSender {
    fn send_request(&self, request: Request) -> Result<()> {
        self.0.send(request).map_err(|_| anyhow!("service is down"))
    }

    pub fn publish_message(&self, message: Message) -> Result<()> {
        self.send_request(Request::PublishMessage { data: message })
    }

    // TODO: expand me
    pub fn sync(&self) -> Result<()> {
        self.send_request(Request::Sync())
    }
}

#[derive(Debug, Decode, Encode)]
pub enum Message {
    ApproveCommitments {
        codes: Option<(Digest, Signature)>,
        blocks: Option<(Digest, Signature)>,
    },
    PublishCommitments {
        codes: Option<AggregatedCommitments<CodeCommitment>>,
        blocks: Option<AggregatedCommitments<BlockCommitment>>,
    },
    RequestCommitmentsValidation {
        codes: Vec<CodeCommitment>,
        blocks: Vec<BlockCommitmentValidationRequest>,
    },
}
