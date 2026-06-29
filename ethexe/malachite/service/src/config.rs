// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Top-level configuration of the [`crate::MalachiteService`].
//!
//! User-facing knobs (listen address, persistent peers, gas allowance,
//! quarantine depth, **validator set**) live here. The validator set
//! is wired in directly — there is no separate genesis file — so the
//! caller is the single source of truth for who can vote.

use crate::Mempool;
use ethexe_common::ecdsa::{PublicKey, Signer};
pub use ethexe_malachite_core::{Multiaddr, ValidatorEntry};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Clone, Debug)]
pub struct MalachiteServiceConfig {
    pub env: MalachiteConfigEnvironment,
    /// Gas allowance per block.
    pub gas_allowance: u64,

    /// Number of canonical descendants an EB must have before it is
    /// out of quarantine and safe to advance to.
    pub canonical_quarantine: u8,

    /// Extra depth on top of `canonical_quarantine` the proposer waits
    /// before choosing an EB to advance to, giving lagging validators
    /// time to sync it.
    // TODO: #5478 reject unreasonable values at config load — `u32::MAX`
    //       turns the producer's anchor walk into millions of RocksDB reads
    //       per `wait_for_proposable_content` invocation.
    pub post_quarantine_delay: u32,

    /// Local libp2p listen address for the Malachite swarm.
    pub listen_addr: SocketAddr,

    /// Directory for the consensus core's WAL and RocksDB store.
    pub home_dir: PathBuf,

    /// Multiaddrs of peers to keep persistent connections to; each entry
    /// must include a `/p2p/<peer_id>` suffix (discovery is off).
    pub persistent_peers: Vec<Multiaddr>,

    /// The complete validator set. Quorum is `> 2/3` of total voting power.
    /// A validator node's own public key must appear in this list.
    pub validators: Vec<ValidatorEntry>,

    /// How long the proposer may wait for proposable content before the
    /// round times out and rotates.
    pub propose_timeout: Duration,
}

impl MalachiteServiceConfig {
    pub const DEFAULT_GAS_ALLOWANCE: u64 = ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;
    /// Default matches [`ethexe_common::gear::CANONICAL_QUARANTINE`].
    pub const DEFAULT_CANONICAL_QUARANTINE: u8 = ethexe_common::gear::CANONICAL_QUARANTINE;
    /// One block is enough to absorb the typical observer skew between validators.
    pub const DEFAULT_POST_QUARANTINE_DELAY: u32 = 1;
    /// TCP listener next to the typical ethexe-network 20333/udp QUIC port.
    pub const DEFAULT_LISTEN_ADDR: SocketAddr = SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        20334,
    );
    /// Ethereum block time occasionally stretches past one slot, so give
    /// the producer more than `SLOT_DURATION` to find a fresh EB.
    pub const DEFAULT_PROPOSE_TIMEOUT: Duration =
        Duration::from_secs(2 * alloy::eips::merge::SLOT_DURATION.as_secs());

    /// Build a config with defaults from the node's home directory.
    /// The validator set is left empty — fill it in with [`Self::with_validators`].
    pub fn from_home_dir(home_dir: PathBuf) -> Self {
        Self {
            env: Default::default(),
            gas_allowance: Self::DEFAULT_GAS_ALLOWANCE,
            canonical_quarantine: Self::DEFAULT_CANONICAL_QUARANTINE,
            post_quarantine_delay: Self::DEFAULT_POST_QUARANTINE_DELAY,
            listen_addr: Self::DEFAULT_LISTEN_ADDR,
            home_dir,
            persistent_peers: Vec::new(),
            validators: Vec::new(),
            propose_timeout: Self::DEFAULT_PROPOSE_TIMEOUT,
        }
    }

    /// Replace the Malachite libp2p listen address.
    #[must_use]
    pub fn with_listen_addr(mut self, addr: SocketAddr) -> Self {
        self.listen_addr = addr;
        self
    }

    /// Replace the Malachite persistent peers list.
    #[must_use]
    pub fn with_persistent_peers(mut self, peers: Vec<Multiaddr>) -> Self {
        self.persistent_peers = peers;
        self
    }

    /// Replace the validator set.
    #[must_use]
    pub fn with_validators(mut self, validators: Vec<ValidatorEntry>) -> Self {
        self.validators = validators;
        self
    }

    pub fn with_gas_allowance(mut self, gas_allowance: u64) -> Self {
        self.gas_allowance = gas_allowance;
        self
    }

    pub fn with_canonical_quarantine(mut self, canonical_quarantine: u8) -> Self {
        self.canonical_quarantine = canonical_quarantine;
        self
    }

    pub fn with_post_quarantine_delay(mut self, post_quarantine_delay: u32) -> Self {
        self.post_quarantine_delay = post_quarantine_delay;
        self
    }
}

/// Validator-only part of the service configuration.
pub struct ValidatorConfig<M: Mempool> {
    /// Validator's on-chain public key; its secret must be in `signer`.
    pub pub_key: PublicKey,
    /// Mempool serving injected transactions to the producer.
    pub mempool: M,
    /// Keystore holding the validator's signing key.
    pub signer: Signer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_home_dir_default_listen_addr() {
        let cfg = MalachiteServiceConfig::from_home_dir(PathBuf::from("/tmp"));
        assert_eq!(cfg.listen_addr, MalachiteServiceConfig::DEFAULT_LISTEN_ADDR);
        assert!(cfg.persistent_peers.is_empty());
        assert!(cfg.validators.is_empty());
    }
}
