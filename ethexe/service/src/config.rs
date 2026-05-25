// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Application config in one place.

use anyhow::Result;
use ethexe_malachite::Multiaddr;
use ethexe_network::NetworkConfig;
use ethexe_prometheus::PrometheusConfig;
use ethexe_rpc::RpcConfig;
use gsigner::secp256k1::{Address, PublicKey};
use std::{collections::BTreeMap, net::SocketAddr, path::PathBuf, str::FromStr, time::Duration};

#[derive(Debug)]
pub struct Config {
    pub node: NodeConfig,
    pub ethereum: EthereumConfig,
    pub network: Option<NetworkConfig>,
    pub malachite: MalachiteCliConfig,
    pub rpc: Option<RpcConfig>,
    pub prometheus: Option<PrometheusConfig>,
}

/// User-facing subset of [`ethexe_malachite::MalachiteConfig`],
/// resolved at CLI/TOML parse time. The rest of the runtime fields
/// (home directory, mempool) are filled in by the service itself.
#[derive(Clone, Debug)]
pub struct MalachiteCliConfig {
    /// Listen address for the Malachite libp2p TCP swarm.
    pub listen_addr: SocketAddr,
    /// Persistent peers the local Malachite swarm should always
    /// connect to. Each entry must include a `/p2p/<peer_id>` suffix.
    /// Discovery is currently disabled, so for a multi-validator
    /// deployment every peer must be listed (or transitively
    /// reachable through the listed ones).
    pub persistent_peers: Vec<Multiaddr>,
    /// Map from validator Ethereum [`Address`] to its Malachite
    /// secp256k1 [`PublicKey`]. The on-chain Router contract stores
    /// the validator set as Ethereum addresses; Malachite needs the
    /// matching public keys to verify votes/proposals. The service
    /// resolves the final validator set by walking the on-chain
    /// validator list (in router order) and looking each address up
    /// in this table, so the table must contain every active
    /// validator's address.
    pub validator_pub_keys: BTreeMap<Address, PublicKey>,
}

impl Default for MalachiteCliConfig {
    fn default() -> Self {
        Self {
            listen_addr: ethexe_malachite::MalachiteConfig::DEFAULT_LISTEN_ADDR,
            persistent_peers: Vec::new(),
            validator_pub_keys: BTreeMap::new(),
        }
    }
}

impl Config {
    pub fn log_info(&self) {
        log::info!("💾 Database: {}", self.node.database_path.display());
        log::info!("🔑 Key directory: {}", self.node.key_path.display());
        log::info!("⧫  Ethereum observer RPC: {}", self.ethereum.rpc);
        log::info!(
            "📡 Ethereum router address: {}",
            self.ethereum.router_address
        );
        if let Some(network) = &self.network {
            log::info!("🛜  Network public key: {}", network.public_key);
        }
    }
}

#[derive(Debug)]
pub struct NodeConfig {
    pub database_path: PathBuf,
    pub key_path: PathBuf,
    pub validator: ConfigPublicKey,
    pub validator_session: ConfigPublicKey,
    pub eth_max_sync_depth: u32,
    pub worker_threads: Option<usize>,
    pub blocking_threads: Option<usize>,
    pub chunk_processing_threads: usize,
    pub block_gas_limit: u64,
    pub batch_size_limit: u64,
    pub canonical_quarantine: u8,
    /// Extra anchor-depth slack the proposer adds on top of
    /// `canonical_quarantine`. See
    /// [`ethexe_malachite::MalachiteConfig::post_quarantine_delay`].
    pub post_quarantine_delay: u32,
    pub dev: bool,
    pub pre_funded_accounts: u32,
    pub fast_sync: bool,
    /// How long the coordinator should wait between observing a new
    /// Ethereum chain head and starting batch aggregation. Buys time for
    /// participants to receive the same chain head and lets the previous
    /// MB finish executing.
    pub coordinator_aggregation_delay: Duration,
    /// Coordinator-local: how many Ethereum blocks the resulting
    /// `BatchCommitment` stays valid past its target block. Encoded into
    /// `BatchCommitment::expiry`.
    pub commitment_delay_limit: std::num::NonZero<u8>,
    /// Force a checkpoint chain commitment when the producer's
    /// `last_advanced_eth_block` runs ahead of `last_committed_eb`
    /// by more than this many Eth blocks.
    pub uncommitted_chain_len_threshold: std::num::NonZero<u32>,
    pub genesis_state_dump: Option<PathBuf>,
}

impl NodeConfig {
    /// Return the path to the database for the given router address.
    pub fn database_path_for(&self, router_address: Address) -> PathBuf {
        let mut path = self.database_path.clone();
        path.push(router_address.to_string());
        path
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConfigPublicKey {
    Enabled(PublicKey),
    Random,
    #[default]
    Disabled,
}

impl ConfigPublicKey {
    pub fn new(key: &Option<String>) -> Result<Self> {
        match key {
            Some(key) => Self::from_str(key),
            None => Ok(Self::Disabled),
        }
    }
}

impl FromStr for ConfigPublicKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "random" => Ok(Self::Random),
            key => Ok(Self::Enabled(key.parse()?)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EthereumConfig {
    pub rpc: String,
    pub beacon_rpc: String,
    pub router_address: Address,
    pub block_time: Duration,
    pub eip1559_fee_increase_percentage: u64,
    pub eip1559_max_fee_per_gas_in_gwei: u128,
    pub blob_gas_multiplier: u128,
}
