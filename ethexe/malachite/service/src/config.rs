// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Top-level configuration of the [`crate::MalachiteService`].
//!
//! User-facing knobs (persistent peers, gas allowance, quarantine
//! depth, **validator set**) live here. The validator set
//! is wired in directly — there is no separate genesis file — so the
//! caller is the single source of truth for who can vote.

use std::{path::PathBuf, time::Duration};

pub use ethexe_malachite_core::{Multiaddr, ValidatorEntry};

#[derive(Clone, Debug)]
pub struct MalachiteConfig {
    /// Gas allowance per block.
    pub gas_allowance: u64,

    /// Number of canonical descendants an Ethereum block must have
    /// before it is considered out of quarantine and safe to anchor a
    /// sequencer block to.
    pub canonical_quarantine: u8,

    /// Extra depth (in Hoodi blocks, on top of `canonical_quarantine`)
    /// the proposer waits before choosing an EB to advance to. A
    /// positive value gives lagging validators time to receive the EB
    /// before the proposer references it, eliminating the need for
    /// validators to wait on local sync inside `validate_block_above`.
    /// Defaults to 1.
    // TODO: #5478 reject unreasonable values at config load — `u32::MAX`
    //       turns the producer's anchor walk into millions of RocksDB reads
    //       per `wait_for_proposable_content` invocation.
    pub post_quarantine_delay: u32,

    /// Upper bound on how long the proposer waits for fresh local content
    /// before Malachite advances the round.
    pub propose_timeout: Duration,

    /// Directory where the wrapped [`ethexe_malachite_core::MalachiteService`] keeps
    /// its WAL (`malachite/consensus.wal`) and RocksDB store
    /// (`malachite/store.db/`).
    pub home_dir: PathBuf,

    /// Shared-network peers the Malachite consensus lane should keep
    /// connected to. Multiaddrs use the shared `ethexe-network` peer id.
    pub persistent_peers: Vec<Multiaddr>,

    /// The complete validator set. The local node's public key (the
    /// one whose secret comes from the [`gsigner::Signer`] passed to
    /// [`crate::MalachiteService::new`]) must appear in this list, or
    /// service start-up fails.
    ///
    /// Voting power is taken at face value — Tendermint's quorum
    /// threshold is `> 2/3` of the total voting power across the
    /// list.
    pub validators: Vec<ValidatorEntry>,
}

impl MalachiteConfig {
    pub const DEFAULT_GAS_ALLOWANCE: u64 = ethexe_common::DEFAULT_BLOCK_GAS_LIMIT;
    /// Default matches [`ethexe_common::gear::CANONICAL_QUARANTINE`].
    pub const DEFAULT_CANONICAL_QUARANTINE: u8 = ethexe_common::gear::CANONICAL_QUARANTINE;
    /// Default extra anchor-depth slack the proposer adds on top of
    /// `canonical_quarantine`; one Hoodi block is enough to absorb the
    /// typical observer-to-observer skew between validators.
    pub const DEFAULT_POST_QUARANTINE_DELAY: u32 = 1;
    pub const DEFAULT_PROPOSE_TIMEOUT: Duration = alloy::eips::merge::SLOT_DURATION;
    /// Build a config with sane defaults from the node's home
    /// directory. The validator set is left empty — the caller MUST
    /// fill it in before passing to [`crate::MalachiteService::new`]
    /// (see [`Self::with_validators`]).
    pub fn from_home_dir(home_dir: PathBuf) -> Self {
        Self {
            gas_allowance: Self::DEFAULT_GAS_ALLOWANCE,
            canonical_quarantine: Self::DEFAULT_CANONICAL_QUARANTINE,
            post_quarantine_delay: Self::DEFAULT_POST_QUARANTINE_DELAY,
            propose_timeout: Self::DEFAULT_PROPOSE_TIMEOUT,
            home_dir,
            persistent_peers: Vec::new(),
            validators: Vec::new(),
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_home_dir_defaults() {
        let cfg = MalachiteConfig::from_home_dir(PathBuf::from("/tmp"));
        assert!(cfg.persistent_peers.is_empty());
        assert!(cfg.validators.is_empty());
    }
}
