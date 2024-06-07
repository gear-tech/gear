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

use anyhow::Result;
use futures::{select, StreamExt};
use libp2p::{
    connection_limits::ConnectionLimits,
    core::upgrade,
    identify::Info as IdentifyInfo,
    swarm::{Swarm, SwarmBuilder, SwarmEvent, THandlerErr},
    Multiaddr,
};
use log::{debug, error, info, trace};
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    fs,
    num::NonZeroUsize,
    sync::{atomic::AtomicUsize, Arc},
};

/// Network for Hypercore nodes.
pub struct NetworkWorker {
    /// Updated by the `NetworkWorker` and loaded by the `NetworkService`.
    listen_addresses: Arc<Mutex<HashSet<Multiaddr>>>,
    /// Updated by the `NetworkWorker` and loaded by the `NetworkService`.
    num_connected: Arc<AtomicUsize>,
    /// The *actual* network.
    network_service: Swarm<Behaviour>,
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

        let external_addresses = Arc::new(Mutex::new(HashSet::new()));

        // Build the swarm.
        // TODO: Use bandwidth in metrics
        let (mut swarm, _bandwidth): (Swarm<Behaviour>, _) = {
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

                let result = Behaviour::new(
                    user_agent,
                    local_public,
                    external_addresses.clone(),
                    connection_limits,
                );

                match result {
                    Ok(b) => b,
                    Err(e) => return Err(e.into()),
                }
            };

            let builder = SwarmBuilder::with_tokio_executor(transport, behaviour, local_peer_id);
            #[allow(deprecated)]
            let builder = builder
                .substream_upgrade_protocol_override(upgrade::Version::V1Lazy)
                .notify_handler_buffer_size(NonZeroUsize::new(32).expect("32 != 0; qed"))
                .per_connection_event_buffer_size(24)
                .max_negotiating_inbound_streams(2048);

            (builder.build(), bandwidth)
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

        for peer in &network_config.boot_nodes {
            swarm.dial(peer.clone())?;
        }

        let listen_addresses = Arc::new(Mutex::new(HashSet::new()));
        let num_connected = Arc::new(AtomicUsize::new(0));

        Ok(Self {
            listen_addresses,
            num_connected,
            network_service: swarm,
        })
    }

    /// Run the network.
    pub async fn run(mut self) {
        while self.next_action().await {}
    }

    /// Perform one action. Returns `true` if it should be called again.
    ///
    /// Intended for tests only. Use `run`].
    pub async fn next_action(&mut self) -> bool {
        select! {
            // Next event from `Swarm` (the stream guaranteed to never terminate).
            event = self.network_service.select_next_some() => {
                self.handle_swarm_event(event);
            },
        }

        true
    }

    /// Process the next event coming from `Swarm`.
    fn handle_swarm_event(&mut self, event: SwarmEvent<BehaviourOut, THandlerErr<Behaviour>>) {
        match event {
            SwarmEvent::Behaviour(BehaviourOut::PeerIdentify {
                peer_id,
                info:
                    IdentifyInfo {
                        protocol_version,
                        agent_version,
                        mut listen_addrs,
                        protocols: _,
                        ..
                    },
            }) => {
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
        let net = NetworkWorker::new(NetworkConfiguration::new_local())
            .expect("failed to create network service");
    }
}
