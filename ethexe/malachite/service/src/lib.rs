// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Malachite
//!
//! Ethexe-side glue around `ethexe-malachite-core`, the generic Malachite BFT /
//! Tendermint-style consensus engine. BFT voting, gossip, peer discovery, and
//! persistence all live in the core crate; this crate provides the public
//! [`MalachiteService`] facade, the producer-side [`Mempool`] abstraction, per-
//! transaction validity checking, and translation of engine callbacks into
//! [`MalachiteEvent`]s.
//!
//! `ethexe-service` constructs the service at startup and is the sole consumer of
//! its output `Stream` of [`MalachiteEvent`]; it calls `receive_new_eb` on each
//! `ObserverEvent::Block` and `receive_eb_synced` on each
//! `ObserverEvent::BlockSynced`.
//!
//! ## Public API
//!
//! - [`MalachiteService`] (struct) — Public facade; `Stream` + driver methods
//! - [`MalachiteEvent`] (enum) — Output event: proposal, finalization, purged txs
//! - [`CommitCertificate`] (struct) — BFT commit proof attached to `BlockFinalized`
//! - [`MalachiteServiceConfig`] (struct) — Service configuration
//! - [`ValidatorEntry`] (struct) — Single entry in the validator set
//! - [`Mempool`] (trait) — Producer-side injected-tx source
//! - [`InjectedTxMempool`] (struct) — Real mempool implementation
//! - [`TxValidityChecker`] (struct) — Per-tx validity against the MB world
//! - [`TxValidity`] (enum) — Validity verdict: `Valid`, `Duplicate`, `Outdated`, …
//!
//! Driver methods on [`MalachiteService`]: `receive_injected_transaction`,
//! `receive_new_eb`, `receive_eb_synced`, `shutdown`.
//!
//! [`TxValidity`] gates inclusion: a producer drops any non-`Valid` tx when
//! building an MB, and a validator rejects an entire MB that contains one.
//!
//! ## Caller Invariants
//!
//! - Construct with `MalachiteServiceStarter::new(config, validator_config, db,
//!   initial_chain_head)` followed by `.start()`. A `Some(validator_config)`
//!   starts a `Validator` whose key must appear in `config.validators`; `None`
//!   starts a gossip/sync-only `FullNode`. `new` returns `Err` if
//!   `config.validators` is empty or the local key is absent from the signer.
//! - `BlockProposal` is always emitted before the matching `BlockFinalized` for a
//!   height; both series are emitted ancestor-first.
//! - Tendermint's quorum threshold is `> 2/3` of total voting power across the
//!   validator list.
//! - Peer discovery is disabled: every `persistent_peers` multiaddr must include a
//!   `/p2p/<peer_id>` suffix, and every validator must be listed or transitively
//!   reachable through a listed peer.
//! - `Drop` is best-effort; call `shutdown().await` before an immediate restart so
//!   RocksDB locks and sockets release.

mod config;
mod externalities;
mod mempool;
mod quarantine;
mod service;
mod starter;
mod tx_validity;
mod types;

use ethexe_common::injected::PurgedTransaction;
use gprimitives::H256;

pub use crate::{
    config::{MalachiteServiceConfig, ValidatorConfig, ValidatorEntry},
    mempool::{DEFAULT_POOL_CAPACITY, InjectedTxMempool, Mempool, TxInsertionStatus},
    service::MalachiteService,
    starter::MalachiteServiceStarter,
    tx_validity::{MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES, TxValidity, TxValidityChecker},
};
pub use ethexe_malachite_core::{
    Multiaddr, PeerId, derive_libp2p_secret, libp2p_peer_id as malachite_libp2p_peer_id,
};

/// Ethexe-shaped commit certificate.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitCertificate {
    /// Committed MB height.
    pub height: u64,
    /// Blake2b envelope hash of the committed MB.
    pub mb_hash: H256,
    /// Validator signatures backing the commit.
    pub signatures: Vec<Vec<u8>>,
}

/// Output event stream of the Malachite service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MalachiteEvent {
    /// New sequencer block persisted; `mb_hash` is the Blake2b envelope hash.
    BlockProposal { height: u64, mb_hash: H256 },

    /// BFT-committed block; `globals.latest_finalized_mb_hash` now points at it.
    BlockFinalized {
        cert: CommitCertificate,
        height: u64,
        mb_hash: H256,
    },

    /// Transactions that were purged from the mempool.
    PurgedTransactions {
        eb_hash: H256,
        transactions: Vec<PurgedTransaction>,
    },
}

impl std::fmt::Display for MalachiteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockProposal { height, mb_hash } => {
                write!(f, "BlockProposal(height: {height}, mb_hash: {mb_hash})")
            }
            Self::BlockFinalized {
                cert,
                height,
                mb_hash,
            } => write!(
                f,
                "BlockFinalized(height: {}, mb_hash: {}, sigs: {})",
                height,
                mb_hash,
                cert.signatures.len()
            ),
            Self::PurgedTransactions {
                eb_hash,
                transactions,
            } => {
                write!(
                    f,
                    "PurgedTransactions(eb_hash: {eb_hash}, transactions_len: {})",
                    transactions.len()
                )
            }
        }
    }
}
