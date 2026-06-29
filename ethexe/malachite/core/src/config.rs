// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Service configuration.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

pub use malachitebft_app_channel::app::net::Multiaddr;

/// secp256k1 public key of one validator. The on-chain address is derived from
/// it (`keccak256(uncompressed_pubkey[1..])[12..]`). The set is unweighted —
/// every validator carries the same voting power, so the BFT quorum threshold
/// is a plain `> 2/3` of the validator count.
//
// TODO: #5480 if receivers ever need to gate `ReceivedProposalPart` against a
//       validator-peer-id allowlist, this must grow into a struct carrying a
//       `libp2p_peer_id: PeerId` alongside the key (the peer-id is not derivable
//       from `public_key` alone — operators must compute it offline via
//       `libp2p_peer_id(&secret)` and embed it).
pub type ValidatorPublicKey = gsigner::schemes::secp256k1::PublicKey;

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

/// All configuration the service needs to bootstrap the malachite engine.
/// Application-specific knobs live behind [`crate::Externalities`].
#[derive(Clone, Debug)]
pub struct MalachiteCoreConfig {
    /// Local libp2p listen address.
    pub listen_addr: SocketAddr,

    /// Base directory; the service owns `<base>/malachite/` (consensus WAL
    /// + RocksDB store), created on first run and resumed on restarts.
    pub base: PathBuf,

    /// Multiaddrs of peers to keep persistent connections to; each entry
    /// must include a `/p2p/<peer_id>` suffix (discovery is off).
    pub persistent_peers: Vec<Multiaddr>,

    /// This node's secp256k1 secret: libp2p peer identity in both roles,
    /// plus vote / proposal signing in [`NodeRole::Validator`] mode.
    pub validator_secret: gsigner::schemes::secp256k1::PrivateKey,

    /// Validator set the engine uses to drive consensus
    /// (see [`NodeRole`] for local-key membership rules).
    pub validators: Vec<ValidatorPublicKey>,

    /// Whether this node casts votes (`Validator`) or just observes
    /// (`FullNode`).
    pub role: NodeRole,

    /// Upper bound on waiting for [`crate::Externalities::build_block_above`]
    /// before the round rolls over.
    pub propose_timeout: Duration,
}
