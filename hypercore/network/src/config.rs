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

pub use libp2p::{
    build_multiaddr,
    identity::{self, ed25519, Keypair},
    multiaddr, Multiaddr, PeerId,
};
use zeroize::Zeroize;

use std::{
    error::Error,
    fmt, fs,
    future::Future,
    io::{self, Write},
    iter,
    net::Ipv4Addr,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    pin::Pin,
    str::{self, FromStr},
};

pub const DEFAULT_LISTEN_PORT: u16 = 20333;

/// Configuration for the transport layer.
#[derive(Clone, Debug)]
pub enum TransportConfig {
    /// Normal transport mode.
    Normal {
        /// If true, the network will use mDNS to discover other libp2p nodes on the local network
        /// and connect to them if they support the same chain.
        enable_mdns: bool,

        /// If true, allow connecting to private IPv4/IPv6 addresses (as defined in
        /// [RFC1918](https://tools.ietf.org/html/rfc1918)). Irrelevant for addresses that have
        /// been passed in `::sc_network::config::NetworkConfiguration::boot_nodes`.
        allow_private_ip: bool,
    },

    /// Only allow connections within the same process.
    /// Only addresses of the form `/memory/...` will be supported.
    MemoryOnly,
}

/// The configuration of a node's secret key, describing the type of key
/// and how it is obtained. A node's identity keypair is the result of
/// the evaluation of the node key configuration.
#[derive(Clone, Debug)]
pub enum NodeKeyConfig {
    /// A Ed25519 secret key configuration.
    Ed25519(Secret<ed25519::SecretKey>),
}

impl Default for NodeKeyConfig {
    fn default() -> NodeKeyConfig {
        Self::Ed25519(Secret::New)
    }
}

/// The options for obtaining a Ed25519 secret key.
pub type Ed25519Secret = Secret<ed25519::SecretKey>;

/// The configuration options for obtaining a secret key `K`.
#[derive(Clone)]
pub enum Secret<K> {
    /// Use the given secret key `K`.
    Input(K),
    /// Read the secret key from a file. If the file does not exist,
    /// it is created with a newly generated secret key `K`. The format
    /// of the file is determined by `K`:
    ///
    ///   * `ed25519::SecretKey`: An unencoded 32 bytes Ed25519 secret key.
    File(PathBuf),
    /// Always generate a new secret key `K`.
    New,
}

impl<K> fmt::Debug for Secret<K> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Input(_) => f.debug_tuple("Secret::Input").finish(),
            Self::File(path) => f.debug_tuple("Secret::File").field(path).finish(),
            Self::New => f.debug_tuple("Secret::New").finish(),
        }
    }
}

impl NodeKeyConfig {
    /// Evaluate a `NodeKeyConfig` to obtain an identity `Keypair`:
    ///
    ///  * If the secret is configured as input, the corresponding keypair is returned.
    ///
    ///  * If the secret is configured as a file, it is read from that file, if it exists. Otherwise
    ///    a new secret is generated and stored. In either case, the keypair obtained from the
    ///    secret is returned.
    ///
    ///  * If the secret is configured to be new, it is generated and the corresponding keypair is
    ///    returned.
    pub fn into_keypair(self) -> io::Result<Keypair> {
        use NodeKeyConfig::*;
        match self {
            Ed25519(Secret::New) => Ok(Keypair::generate_ed25519()),

            Ed25519(Secret::Input(k)) => Ok(ed25519::Keypair::from(k).into()),

            Ed25519(Secret::File(f)) => get_secret(
                f,
                |mut b| match String::from_utf8(b.to_vec()).ok().and_then(|s| {
                    if s.len() == 64 {
                        array_bytes::hex2bytes(&s).ok()
                    } else {
                        None
                    }
                }) {
                    Some(s) => ed25519::SecretKey::try_from_bytes(s),
                    _ => ed25519::SecretKey::try_from_bytes(&mut b),
                },
                ed25519::SecretKey::generate,
                |b| b.as_ref().to_vec(),
            )
            .map(ed25519::Keypair::from)
            .map(Keypair::from),
        }
    }
}

/// Load a secret key from a file, if it exists, or generate a
/// new secret key and write it to that file. In either case,
/// the secret key is returned.
fn get_secret<P, F, G, E, W, K>(file: P, parse: F, generate: G, serialize: W) -> io::Result<K>
where
    P: AsRef<Path>,
    F: for<'r> FnOnce(&'r mut [u8]) -> Result<K, E>,
    G: FnOnce() -> K,
    E: Error + Send + Sync + 'static,
    W: Fn(&K) -> Vec<u8>,
{
    std::fs::read(&file)
        .and_then(|mut sk_bytes| {
            parse(&mut sk_bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
        .or_else(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                file.as_ref().parent().map_or(Ok(()), fs::create_dir_all)?;
                let sk = generate();
                let mut sk_vec = serialize(&sk);
                write_secret_file(file, &sk_vec)?;
                sk_vec.zeroize();
                Ok(sk)
            } else {
                Err(e)
            }
        })
}

/// Write secret bytes to a file.
fn write_secret_file<P>(path: P, sk_bytes: &[u8]) -> io::Result<()>
where
    P: AsRef<Path>,
{
    let mut file = open_secret_file(&path)?;
    file.write_all(sk_bytes)
}

/// Opens a file containing a secret key in write mode.
#[cfg(unix)]
fn open_secret_file<P>(path: P) -> io::Result<fs::File>
where
    P: AsRef<Path>,
{
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

/// Opens a file containing a secret key in write mode.
#[cfg(not(unix))]
fn open_secret_file<P>(path: P) -> Result<fs::File, io::Error>
where
    P: AsRef<Path>,
{
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Configuration for a set of nodes.
#[derive(Clone, Debug)]
pub struct SetConfig {
    /// List of reserved node addresses.
    pub reserved_nodes: Vec<Multiaddr>,
}

impl Default for SetConfig {
    fn default() -> Self {
        Self {
            reserved_nodes: Vec::new(),
        }
    }
}

/// Network service configuration.
#[derive(Clone, Debug)]
pub struct NetworkConfiguration {
    /// Directory path to store network-specific configuration. None means nothing will be saved.
    pub net_config_path: Option<PathBuf>,

    /// Multiaddresses to listen for incoming connections.
    pub listen_addresses: Vec<Multiaddr>,

    /// Multiaddresses to advertise. Detected automatically if empty.
    pub public_addresses: Vec<Multiaddr>,

    /// List of initial node addresses
    pub boot_nodes: Vec<Multiaddr>,

    /// The node key configuration, which determines the node's network identity keypair.
    pub node_key: NodeKeyConfig,

    /// Configuration for the default set of nodes used for block syncing and transactions.
    pub default_peers_set: SetConfig,

    /// Name of the node. Sent over the wire for debugging purposes.
    pub node_name: String,

    /// Configuration for the transport layer.
    pub transport: TransportConfig,
}

impl NetworkConfiguration {
    /// Create new default configuration
    pub fn new<SN: Into<String>>(
        node_name: SN,
        node_key: NodeKeyConfig,
        net_config_path: Option<PathBuf>,
    ) -> Self {
        let default_peers_set = SetConfig::default();
        Self {
            net_config_path,
            listen_addresses: Vec::new(),
            public_addresses: Vec::new(),
            boot_nodes: Vec::new(),
            node_key,
            default_peers_set,
            node_name: node_name.into(),
            transport: TransportConfig::Normal {
                enable_mdns: false,
                allow_private_ip: true,
            },
        }
    }

    /// Create new default configuration for localhost-only connection with random port (useful for
    /// testing)
    pub fn new_local() -> NetworkConfiguration {
        let mut config = NetworkConfiguration::new("test-node", Default::default(), None);

        config.listen_addresses =
            vec![
                iter::once(multiaddr::Protocol::Ip4(Ipv4Addr::new(127, 0, 0, 1)))
                    .chain(iter::once(multiaddr::Protocol::Tcp(0)))
                    .collect(),
            ];

        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tempdir_with_prefix(prefix: &str) -> TempDir {
        tempfile::Builder::new().prefix(prefix).tempdir().unwrap()
    }

    fn secret_bytes(kp: Keypair) -> Vec<u8> {
        kp.try_into_ed25519()
            .expect("ed25519 keypair")
            .secret()
            .as_ref()
            .iter()
            .cloned()
            .collect()
    }

    #[test]
    fn test_secret_file() {
        let tmp = tempdir_with_prefix("x");
        std::fs::remove_dir(tmp.path()).unwrap(); // should be recreated
        let file = tmp.path().join("x").to_path_buf();
        let kp1 = NodeKeyConfig::Ed25519(Secret::File(file.clone()))
            .into_keypair()
            .unwrap();
        let kp2 = NodeKeyConfig::Ed25519(Secret::File(file.clone()))
            .into_keypair()
            .unwrap();
        assert!(file.is_file() && secret_bytes(kp1) == secret_bytes(kp2))
    }

    #[test]
    fn test_secret_input() {
        let sk = ed25519::SecretKey::generate();
        let kp1 = NodeKeyConfig::Ed25519(Secret::Input(sk.clone()))
            .into_keypair()
            .unwrap();
        let kp2 = NodeKeyConfig::Ed25519(Secret::Input(sk))
            .into_keypair()
            .unwrap();
        assert!(secret_bytes(kp1) == secret_bytes(kp2));
    }

    #[test]
    fn test_secret_new() {
        let kp1 = NodeKeyConfig::Ed25519(Secret::New).into_keypair().unwrap();
        let kp2 = NodeKeyConfig::Ed25519(Secret::New).into_keypair().unwrap();
        assert!(secret_bytes(kp1) != secret_bytes(kp2));
    }
}
