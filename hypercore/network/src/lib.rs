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

//! Network library for Hypercore.

pub mod config;
pub mod error;

pub use crate::{
    config::{DEFAULT_LISTEN_PORT, *},
    error::Error,
};
#[doc(inline)]
pub use libp2p::{multiaddr, Multiaddr, PeerId};

use anyhow::{Context, Result};
use futures::{AsyncReadExt, AsyncWriteExt, StreamExt};
use libp2p::{
    gossipsub, mdns,
    multiaddr::Protocol,
    noise,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Stream, StreamProtocol, Swarm,
};
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    time::Duration,
};
use tokio::{
    io,
    io::AsyncBufReadExt,
    select,
    sync::{mpsc, oneshot},
};
use tracing_subscriber::EnvFilter;

const LOG_TARGET: &str = "hyp-libp2p";

// We create a custom network behaviour that combines Gossipsub and Mdns.
#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
}
/// Network for Hypercore nodes.
pub struct Network {
    swarm: Swarm<MyBehaviour>,
}

impl Network {
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

        log::info!(
            target: "sub-libp2p",
            "🏷  Local node identity is: {}",
            local_peer_id.to_base58(),
        );

        let mut swarm = libp2p::SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| {
                // To content-address message, we can take the hash of message and use it as an ID.
                let message_id_fn = |message: &gossipsub::Message| {
                    let mut s = DefaultHasher::new();
                    message.data.hash(&mut s);
                    gossipsub::MessageId::from(s.finish().to_string())
                };

                // Set a custom gossipsub configuration
                let gossipsub_config = gossipsub::ConfigBuilder::default()
                    .heartbeat_interval(Duration::from_secs(10)) // This is set to aid debugging by not cluttering the log space
                    .validation_mode(gossipsub::ValidationMode::Strict) // This sets the kind of message validation. The default is Strict (enforce message signing)
                    .message_id_fn(message_id_fn) // content-address messages. No two messages of the same content will be propagated.
                    .build()
                    .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; // Temporary hack because `build` does not return a proper `std::error::Error`.

                // build a gossipsub network behaviour
                let gossipsub = gossipsub::Behaviour::new(
                    gossipsub::MessageAuthenticity::Signed(key.clone()),
                    gossipsub_config,
                )?;

                Ok(MyBehaviour { gossipsub })
            })?
            .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        // Create a Gossipsub topic
        let topic = gossipsub::IdentTopic::new("gpu-validation");

        // subscribes to our topic
        swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

        // Listen on multiaddresses.
        for addr in &network_config.listen_addresses {
            if let Err(err) = Swarm::<MyBehaviour>::listen_on(&mut swarm, addr.clone()) {
                log::warn!(target: LOG_TARGET, "Can't listen on {} because: {:?}", addr, err)
            }
        }

        // Add external addresses.
        for addr in &network_config.public_addresses {
            Swarm::<MyBehaviour>::add_external_address(&mut swarm, addr.clone());
        }

        for peer in &network_config.boot_nodes {
            swarm.dial(peer.clone())?;
        }

        Ok(Self { swarm })
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
            event = self.swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: peer_id,
                    message_id: id,
                    message,
                })) => log::info!(
                        "Got message: '{}' with id: {id} from peer: {peer_id}",
                        String::from_utf8_lossy(&message.data),
                    ),
                SwarmEvent::NewListenAddr { address, .. } => {
                    log::info!("Local node is listening on {address}");
                },
                SwarmEvent::IncomingConnection { local_addr, send_back_addr, .. } => {

                    log::info!("IncomingConnection on {local_addr} from {send_back_addr}");
                }
                _ => {}
            }
        }
        true
    }
}

impl Unpin for Network {}

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
        let net = Network::new(NetworkConfiguration::new_local()).expect("failed to create network service");

    }
}
