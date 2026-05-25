// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Service configuration.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

pub use malachitebft_app_channel::app::net::Multiaddr;

/// One entry of the validator set. The set is fixed for the lifetime
/// of the deployment — to rotate validators every node must be
/// re-bootstrapped from a fresh [`MalachiteConfig`].
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
///
/// A `FullNode` doesn't propose or vote — it joins the gossip mesh,
/// receives proposals + sync responses, and surfaces them to the
/// application via [`crate::Externalities::process_mb_proposal`] /
/// [`crate::Externalities::process_mb_finalized`] just like a
/// validator would. Use this for read-only observers,
/// quarantine workers, light clients, etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeRole {
    /// Sign votes and proposals; broadcast a validator proof on
    /// connect; the local address must appear in [`MalachiteConfig::validators`].
    Validator,
    /// Read-only participant — joins gossip / sync, validates
    /// incoming blocks, but never signs anything. The local address
    /// must NOT appear in [`MalachiteConfig::validators`].
    FullNode,
}

/// All configuration the service needs to bootstrap the malachite
/// engine.
///
/// Application-specific knobs (gas budgets, mempool settings, etc.)
/// live behind [`crate::Externalities`] — they don't belong here.
#[derive(Clone, Debug)]
pub struct MalachiteConfig {
    /// Local libp2p listen address.
    pub listen_addr: SocketAddr,

    /// Application's project base directory. The service carves out
    /// `<base>/malachite/` and owns everything inside it: the
    /// consensus WAL (`consensus.wal`) and the RocksDB store
    /// (`store.db/` — block entries, decided/undecided proposals,
    /// pending parts, height index, engine certificates). Anything
    /// else under `base` is the application's business.
    ///
    /// The artifacts inside `<base>/malachite/` are created on first
    /// run; subsequent runs resume from where the previous one left
    /// off.
    ///
    /// In tests, the caller is responsible for keeping this directory
    /// alive across service restarts (don't drop the `TempDir` between
    /// service spawns).
    pub base: PathBuf,

    /// Multiaddrs the local node should keep persistent connections
    /// to. Each entry must include the `/p2p/<peer_id>` suffix so the
    /// swarm knows who to expect on the other side. Discovery is off,
    /// so multi-validator deployments need every node's multiaddr
    /// listed (or at least transitively reachable).
    pub persistent_peers: Vec<Multiaddr>,

    /// This node's secp256k1 secret. Used (after a domain-separated
    /// derivation) for the libp2p peer identity in both roles, and
    /// additionally for malachite vote / proposal signing in
    /// [`NodeRole::Validator`] mode.
    pub validator_secret: gsigner::schemes::secp256k1::PrivateKey,

    /// Validator set the engine uses to drive consensus. For
    /// [`NodeRole::Validator`] the set must contain an entry whose
    /// public key matches [`Self::validator_secret`]; for
    /// [`NodeRole::FullNode`] the local key must NOT be in the set.
    pub validators: Vec<ValidatorEntry>,

    /// Whether this node casts votes (`Validator`) or just observes
    /// (`FullNode`).
    pub role: NodeRole,

    /// Upper bound on how long the service will wait on
    /// [`crate::Externalities::build_block_above`] before giving up
    /// and letting malachite's round timeout advance the proposer.
    pub propose_timeout: Duration,
}

impl MalachiteConfig {
    /// Default propose timeout — 13 seconds. The upper bound on how
    /// long [`crate::Externalities::build_block_above`] is given to
    /// produce a block before the round rolls over. Applications
    /// should override this when they have a faster or slower
    /// block-production deadline.
    pub const DEFAULT_PROPOSE_TIMEOUT: Duration = Duration::from_secs(13);

    /// Default libp2p listen address — `0.0.0.0:20334`. Sits next to
    /// the typical 20333/udp QUIC port commonly used for
    /// application-level networking, but on TCP since malachite's
    /// default transport is TCP.
    pub const DEFAULT_LISTEN_ADDR: SocketAddr = SocketAddr::new(
        std::net::IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0)),
        20334,
    );
}
