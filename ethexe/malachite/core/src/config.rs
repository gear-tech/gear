// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Service configuration.

use std::{path::PathBuf, time::Duration};

pub use malachitebft_app_channel::app::net::Multiaddr;

/// One entry of the validator set.
//
// TODO: #5480 add `libp2p_peer_id: PeerId` so receivers can gate
//       `ReceivedProposalPart` against a validator-peer-id allowlist
//       (libp2p peer-id is not derivable from `public_key` alone — operators
//       must compute it offline via `libp2p_peer_id(&secret)` and embed it).
#[derive(Clone, Debug)]
pub struct ValidatorEntry {
    /// secp256k1 public key for this validator. The on-chain address
    /// is derived from it (`keccak256(uncompressed_pubkey[1..])[12..]`).
    pub public_key: gsigner::schemes::secp256k1::PublicKey,
    /// Voting power. Must be > 0; the BFT quorum threshold is
    /// `> 2/3` of the total voting power across the set.
    pub voting_power: u64,
}

/// Role this node plays in the BFT swarm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRole {
    /// Signs votes and proposals; the local key must appear in
    /// [`MalachiteCoreConfig::validators`].
    Validator,
    /// Read-only participant — joins gossip / sync and validates blocks,
    /// but never signs; the local key must NOT be in the validator set.
    FullNode,
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Environment {
    #[default]
    Production,
    Test,
}

/// All configuration the service needs to bootstrap the malachite engine.
/// Application-specific knobs live behind [`crate::Externalities`].
#[derive(Clone, Debug)]
pub struct MalachiteCoreConfig {
    pub env: Environment,
    /// Base directory; the service owns `<base>/malachite/` (consensus WAL
    /// + RocksDB store), created on first run and resumed on restarts.
    pub base: PathBuf,

    /// This node's secp256k1 secret: libp2p peer identity in both roles,
    /// plus vote / proposal signing in [`NodeRole::Validator`] mode.
    pub validator_secret: gsigner::schemes::secp256k1::PrivateKey,

    /// Validator set the engine uses to drive consensus
    /// (see [`NodeRole`] for local-key membership rules).
    pub validators: Vec<ValidatorEntry>,

    /// Whether this node casts votes (`Validator`) or just observes
    /// (`FullNode`).
    pub role: NodeRole,

    /// Upper bound on waiting for [`crate::Externalities::build_block_above`]
    /// before the round rolls over.
    pub propose_timeout: Duration,
}

impl MalachiteCoreConfig {
    /// Default propose timeout.
    pub const DEFAULT_PROPOSE_TIMEOUT: Duration = Duration::from_secs(13);
}
