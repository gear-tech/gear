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

use crate::{
    behaviour::{Behaviour, BehaviourOut},
    transport, Error, NetworkConfiguration, TransportConfig,
};

use hypercore_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};

use anyhow::Result;
use either::Either;
use futures::{select, Stream, StreamExt};
use libp2p::{
    connection_limits::ConnectionLimits,
    core::upgrade,
    gossipsub,
    identify::Info as IdentifyInfo,
    identity::Keypair,
    swarm::{Config, Swarm, SwarmEvent},
    Multiaddr, PeerId,
};
use log::{debug, error, info, trace};
use parking_lot::Mutex;
use std::{
    collections::{hash_map::DefaultHasher, HashSet},
    fs,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use void::Void;

pub type SwarmEventError = Either<Either<Either<Void, std::io::Error>, Void>, Void>;

#[allow(unused)]
/// hypercore network service. Handles network IO and manages connectivity.
pub struct NetworkService {
    /// The local external addresses.
    external_addresses: Arc<Mutex<HashSet<Multiaddr>>>,
    /// Listen addresses. Do **NOT** include a trailing `/p2p/` with our `PeerId`.
    listen_addresses: Arc<Mutex<HashSet<Multiaddr>>>,
    /// Local copy of the `PeerId` of the local node.
    local_peer_id: PeerId,
    /// The `KeyPair` that defines the `PeerId` of the local node.
    local_identity: Keypair,
    /// Bandwidth logging system. Can be queried to know the average bandwidth consumed.
    bandwidth: Arc<transport::BandwidthSinks>,
    /// Channel that sends messages to the actual worker.
    to_worker: TracingUnboundedSender<ServiceToWorkerMsg>,
}

pub struct GossipMessageStream {
    receiver: TracingUnboundedReceiver<MessageEntry>,
}

impl GossipMessageStream {
    pub fn new(receiver: TracingUnboundedReceiver<MessageEntry>) -> Self {
        GossipMessageStream { receiver }
    }
}

impl Stream for GossipMessageStream {
    type Item = MessageEntry;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.receiver).poll_next(cx)
    }
}

pub trait NetworkGossip {
    fn broadcast_commitments(&self, data: impl Into<Vec<u8>>);
    fn gossip_message_stream(&self) -> GossipMessageStream;
}

impl NetworkGossip for NetworkService {
    fn broadcast_commitments(&self, data: impl Into<Vec<u8>>) {
        let _ = self
            .to_worker
            .unbounded_send(ServiceToWorkerMsg::GossipCommitments { data: data.into() });
    }
    fn gossip_message_stream(&self) -> GossipMessageStream {
        let (tx, rx) = tracing_unbounded("gossip_message_stream", 1000);
        let _ = self
            .to_worker
            .unbounded_send(ServiceToWorkerMsg::GossipMessageStream { sender: tx });
        GossipMessageStream::new(rx)
    }
}

/// Messages sent from the `NetworkService` to the `NetworkWorker`.
///
/// Each entry corresponds to a method of `NetworkService`.
enum ServiceToWorkerMsg {
    GossipCommitments {
        data: Vec<u8>,
    },
    GossipMessageStream {
        sender: TracingUnboundedSender<MessageEntry>,
    },
}

#[derive(Debug, Clone)]
pub struct MessageEntry {
    pub message_id: gossipsub::MessageId,
    pub topic_hash: gossipsub::TopicHash,
    pub data: Vec<u8>,
    pub sender: Option<PeerId>,
}

/// Network for Hypercore nodes.
pub struct NetworkWorker {
    /// Updated by the `NetworkWorker` and loaded by the `NetworkService`.
    listen_addresses: Arc<Mutex<HashSet<Multiaddr>>>,
    /// The network service that can be extracted and shared through the codebase.
    service: Arc<NetworkService>,
    /// The *actual* network.
    network_service: Swarm<Behaviour>,
    /// Messages from the [`NetworkService`] that must be processed.
    from_service: TracingUnboundedReceiver<ServiceToWorkerMsg>,
    /// Messages received from gossip engine
    gossip_message_stream: Option<TracingUnboundedSender<MessageEntry>>,
}

impl NetworkWorker {
    //! Start the networking service
    pub fn new(network_config: NetworkConfiguration) -> Result<Self> {
        // Private and public keys configuration.
        let local_identity = network_config.node_key.clone().into_keypair()?;
        let local_public = local_identity.public();
        let local_peer_id = local_public.to_peer_id();

        // Ensure the listen addresses are consistent with the transport.
        ensure_addresses_consistent_with_transport(
            network_config.listen_addresses.iter(),
            &network_config.transport,
        )?;
        ensure_addresses_consistent_with_transport(
            network_config.boot_nodes.iter(),
            &network_config.transport,
        )?;
        ensure_addresses_consistent_with_transport(
            network_config.default_peers_set.reserved_nodes.iter(),
            &network_config.transport,
        )?;
        ensure_addresses_consistent_with_transport(
            network_config.public_addresses.iter(),
            &network_config.transport,
        )?;

        let (to_worker, from_service) = tracing_unbounded("mpsc_network_worker", 100_000);

        if let Some(path) = &network_config.net_config_path {
            fs::create_dir_all(path)?;
        }

        log::info!("🏷  Local node identity is: {}", local_peer_id.to_base58(),);

        let (transport, bandwidth) = {
            let config_mem = match network_config.transport {
                TransportConfig::MemoryOnly => true,
                TransportConfig::Normal { .. } => false,
            };

            transport::build_transport(local_identity.clone(), config_mem)
        };

        let known_addresses = {
            // Collect all reserved nodes and bootnodes addresses.
            let mut addresses: Vec<_> = network_config
                .default_peers_set
                .reserved_nodes
                .iter()
                .chain(network_config.boot_nodes.iter())
                .collect();

            // Remove possible duplicates.
            addresses.sort();
            addresses.dedup();

            addresses
        };

        let listen_addresses = Arc::new(Mutex::new(HashSet::new()));

        let external_addresses = Arc::new(Mutex::new(HashSet::new()));

        // Build the swarm.
        // TODO: Use bandwidth in metrics
        let (mut swarm, bandwidth): (Swarm<Behaviour>, _) = {
            // TODO: Add client version
            let user_agent = network_config.node_name.to_string();

            let behaviour = {
                let connection_limits = {
                    let limits = ConnectionLimits::default()
                        .with_max_established_per_peer(Some(crate::MAX_CONNECTIONS_PER_PEER as u32))
                        .with_max_established_incoming(Some(
                            crate::MAX_CONNECTIONS_ESTABLISHED_INCOMING,
                        ));
                    libp2p::connection_limits::Behaviour::new(limits)
                };

                let gossipsub = {
                    // To content-address message, we can take the hash of message and use it as an ID.
                    let message_id_fn = |message: &gossipsub::Message| {
                        let mut s = DefaultHasher::new();
                        message.data.hash(&mut s);
                        gossipsub::MessageId::from(s.finish().to_string())
                    };

                    // Set a custom gossipsub configuration
                    let gossipsub_config = gossipsub::ConfigBuilder::default()
                        // This is set to aid debugging by not cluttering the log space
                        .heartbeat_interval(Duration::from_secs(5))
                        // This sets the kind of message validation. The default is Strict (enforce message signing)
                        .validation_mode(gossipsub::ValidationMode::Strict)
                        // content-address messages. No two messages of the same content will be propagated.
                        .message_id_fn(message_id_fn)
                        .build()
                        .expect("Valid config");

                    // build a gossipsub network behaviour
                    let mut gossipsub = gossipsub::Behaviour::new(
                        gossipsub::MessageAuthenticity::Signed(local_identity.clone()),
                        gossipsub_config,
                    )
                    .expect("Correct configuration");
                    // Create a Gossipsub topic
                    let topic = gossipsub::IdentTopic::new("gpu-commitments");

                    // subscribes to our topic
                    gossipsub.subscribe(&topic)?;
                    gossipsub
                };

                let result = Behaviour::new(
                    user_agent,
                    local_public,
                    external_addresses.clone(),
                    connection_limits,
                    gossipsub,
                );

                match result {
                    Ok(b) => b,
                    Err(e) => return Err(e.into()),
                }
            };

            let config = Config::with_tokio_executor()
                .with_substream_upgrade_protocol_override(upgrade::Version::V1Lazy)
                .with_notify_handler_buffer_size(NonZeroUsize::new(32).expect("32 != 0; qed"))
                .with_per_connection_event_buffer_size(24)
                .with_max_negotiating_inbound_streams(2048);

            let swarm = Swarm::new(transport, behaviour, local_peer_id, config);

            (swarm, bandwidth)
        };

        // Listen on multiaddresses.
        for addr in &network_config.listen_addresses {
            if let Err(err) = Swarm::<Behaviour>::listen_on(&mut swarm, addr.clone()) {
                log::warn!("Can't listen on {} because: {:?}", addr, err)
            }
        }

        // Add external addresses.
        for addr in &network_config.public_addresses {
            Swarm::<Behaviour>::add_external_address(&mut swarm, addr.clone());
        }

        for address in known_addresses {
            swarm.dial(address.clone())?;
        }

        let service = Arc::new(NetworkService {
            bandwidth,
            external_addresses,
            listen_addresses: listen_addresses.clone(),
            local_peer_id,
            local_identity,
            to_worker,
        });

        Ok(Self {
            listen_addresses,
            service,
            network_service: swarm,
            from_service,
            gossip_message_stream: None,
        })
    }

    /// Return a `NetworkService` that can be shared through the code base and can be used to
    /// manipulate the worker.
    pub fn service(&self) -> &Arc<NetworkService> {
        &self.service
    }

    /// Run the network.
    pub async fn run(mut self) {
        while self.next_action().await {}
    }

    // TODO: handle connection closing manually.
    /// Perform one action. Returns `true` if it should be called again.
    ///
    /// Intended for tests only. Use `run`].
    pub async fn next_action(&mut self) -> bool {
        select! {
            // Next message from the service.
            msg = self.from_service.next() => {
                if let Some(msg) = msg {
                    self.handle_worker_message(msg);
                } else {
                    return false
                }
            },
            // Next event from `Swarm` (the stream guaranteed to never terminate).
            event = self.network_service.select_next_some() => {
                self.handle_swarm_event(event);
            },
        }

        true
    }

    /// Process the next message coming from the `NetworkService`.
    fn handle_worker_message(&mut self, msg: ServiceToWorkerMsg) {
        match msg {
            ServiceToWorkerMsg::GossipCommitments { data } => {
                let topic = "gpu-commitments".to_string();
                self.network_service
                    .behaviour_mut()
                    .gossipsub_publish(topic, data)
            }
            ServiceToWorkerMsg::GossipMessageStream { sender } => {
                self.gossip_message_stream = Some(sender);
            }
        }
    }

    /// Process the next event coming from `Swarm`.
    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourOut, SwarmEventError>) {
        match event {
            SwarmEvent::Behaviour(BehaviourOut::GossipCommitments {
                message_id,
                message,
                ..
            }) => {
                debug!(
                    "Libp2p::gossipsub => GossipCommitments from {:?}",
                    message.source
                );
                let entry = MessageEntry {
                    message_id,
                    topic_hash: message.topic,
                    data: message.data,
                    sender: message.source,
                };
                if let Some(ref sender) = self.gossip_message_stream {
                    let _ = sender.unbounded_send(entry);
                }
            }
            SwarmEvent::Behaviour(BehaviourOut::PeerIdentify { peer_id, info }) => {
                let IdentifyInfo {
                    protocol_version,
                    agent_version,
                    mut listen_addrs,
                    protocols: _,
                    ..
                } = *info;

                if listen_addrs.len() > 30 {
                    debug!(

                        "Node {:?} has reported more than 30 addresses; it is identified by {:?} and {:?}",
                        peer_id, protocol_version, agent_version
                    );
                    listen_addrs.truncate(30);
                }
            }
            SwarmEvent::Behaviour(BehaviourOut::None) => {
                // Ignored event from lower layers.
            }
            SwarmEvent::ConnectionEstablished {
                peer_id,
                endpoint: _,
                num_established: _,
                concurrent_dial_errors,
                ..
            } => {
                if let Some(errors) = concurrent_dial_errors {
                    debug!(
                        "Libp2p => Connected({:?}) with errors: {:?}",
                        peer_id, errors
                    );
                } else {
                    debug!("Libp2p => Connected({:?})", peer_id);
                }
            }
            SwarmEvent::ConnectionClosed {
                connection_id: _,
                peer_id,
                cause,
                endpoint: _,
                num_established: _,
            } => {
                debug!("Libp2p => Disconnected({:?}, {:?})", peer_id, cause);
            }
            SwarmEvent::NewListenAddr { address, .. } => {
                trace!("Libp2p => NewListenAddr({})", address);
                self.listen_addresses.lock().insert(address.clone());
            }
            SwarmEvent::ExpiredListenAddr { address, .. } => {
                info!("📪 No longer listening on {}", address);
                self.listen_addresses.lock().remove(&address);
            }
            SwarmEvent::OutgoingConnectionError {
                connection_id: _,
                peer_id,
                error,
            } => {
                if let Some(peer_id) = peer_id {
                    trace!("Libp2p => Failed to reach {:?}: {}", peer_id, error,);
                }
            }
            SwarmEvent::Dialing {
                peer_id,
                connection_id: _,
            } => {
                trace!("Libp2p => Dialing({:?})", peer_id)
            }
            SwarmEvent::IncomingConnection {
                connection_id: _,
                local_addr,
                send_back_addr,
            } => {
                trace!(
                    "Libp2p => IncomingConnection({},{}))",
                    local_addr,
                    send_back_addr
                );
            }
            SwarmEvent::IncomingConnectionError {
                connection_id: _,
                local_addr,
                send_back_addr,
                error,
            } => {
                debug!(
                    "Libp2p => IncomingConnectionError({},{}): {}",
                    local_addr, send_back_addr, error,
                );
            }
            SwarmEvent::ListenerClosed {
                reason, addresses, ..
            } => {
                let mut listen_addresses = self.listen_addresses.lock();
                for addr in &addresses {
                    listen_addresses.remove(addr);
                }
                drop(listen_addresses);

                let addrs = addresses
                    .into_iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                match reason {
                    Ok(()) => error!("📪 Libp2p listener ({}) closed gracefully", addrs),
                    Err(e) => error!("📪 Libp2p listener ({}) closed: {}", addrs, e),
                }
            }
            SwarmEvent::ListenerError { error, .. } => {
                debug!("Libp2p => ListenerError: {}", error);
            }
        }
    }
}

impl Unpin for NetworkWorker {}

fn ensure_addresses_consistent_with_transport<'a>(
    addresses: impl Iterator<Item = &'a Multiaddr>,
    transport: &TransportConfig,
) -> Result<(), Error> {
    if matches!(transport, TransportConfig::MemoryOnly) {
        let addresses: Vec<_> = addresses
            .filter(|x| {
                x.iter()
                    .any(|y| !matches!(y, libp2p::core::multiaddr::Protocol::Memory(_)))
            })
            .cloned()
            .collect();

        if !addresses.is_empty() {
            return Err(Error::AddressesForAnotherTransport {
                transport: transport.clone(),
                addresses,
            });
        }
    } else {
        let addresses: Vec<_> = addresses
            .filter(|x| {
                x.iter()
                    .any(|y| matches!(y, libp2p::core::multiaddr::Protocol::Memory(_)))
            })
            .cloned()
            .collect();

        if !addresses.is_empty() {
            return Err(Error::AddressesForAnotherTransport {
                transport: transport.clone(),
                addresses,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy() {
        let _net = NetworkWorker::new(NetworkConfiguration::new_local())
            .expect("failed to create network service");
    }
}
