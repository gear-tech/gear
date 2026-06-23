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
//! its output `Stream` of [`MalachiteEvent`]; it calls `receive_new_chain_head`
//! on each `ObserverEvent::BlockSynced`.
//!
//! ## Public API
//!
//! - [`MalachiteService`] (struct) — Public facade; `Stream` + driver methods
//! - [`MalachiteEvent`] (enum) — Output event: proposal, finalization, purged txs
//! - [`CommitCertificate`] (struct) — BFT commit proof attached to `BlockFinalized`
//! - [`MalachiteConfig`] (struct) — Service configuration
//! - [`ValidatorEntry`] (struct) — Single entry in the validator set
//! - [`Mempool`] (trait) — Producer-side injected-tx source
//! - [`InjectedTxMempool`] (struct) — Real mempool implementation
//! - [`TxValidityChecker`] (struct) — Per-tx validity against the MB world
//! - [`TxValidity`] (enum) — Validity verdict: `Valid`, `Duplicate`, `Outdated`, …
//!
//! Driver methods on [`MalachiteService`]: `receive_injected_transaction`,
//! `receive_new_chain_head`, `receive_eb_prepared`, `shutdown`.
//!
//! [`TxValidity`] gates inclusion: a producer drops any non-`Valid` tx when
//! building an MB, and a validator rejects an entire MB that contains one.
//!
//! ## Caller Invariants
//!
//! - Construct with `MalachiteService::new(config, db, signer, validator_pub_key,
//!   validator_tdec_setup, mempool)`. A `Some` key starts a `Validator` and must appear in
//!   `config.validators`; `None` starts a gossip/sync-only `FullNode`. `new`
//!   returns `Err` if `config.validators` is empty or the local key is absent.
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
mod decryption_shares;
mod externalities;
mod mempool;
mod quarantine;
mod service;
mod tx_validity;

pub use crate::{
    config::{MalachiteConfig, ValidatorEntry},
    mempool::{DEFAULT_POOL_CAPACITY, InjectedTxMempool, Mempool, TxInsertionStatus},
    service::{MalachiteService, ValidatorTdecSetup},
    tx_validity::{MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES, TxValidity, TxValidityChecker},
};
pub use ethexe_common::{
    HashOf,
    injected::Transaction,
    malachite::{Operation, Operations},
};
use ethexe_common::{
    injected::{InjectedTransaction, PurgedTransaction, ShieldedTransaction},
    malachite::ShieldedTxDecryptionShare,
};
pub use ethexe_malachite_core::{
    Multiaddr, PeerId, derive_libp2p_secret, libp2p_peer_id as malachite_libp2p_peer_id,
};
pub use gprimitives::H256;

/// Ethexe-shaped commit certificate; `block_hash` is the Blake2b envelope hash.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitCertificate {
    pub height: u64,
    pub mb_hash: H256,
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

    /// Output of unshielding transactions in MB.
    UnshieldingOutput {
        mb_hash: H256,
        /// Mapping from shielded transaction hashes to unshielded transaction hashes.
        unshielded_hash_mapping: Vec<(HashOf<ShieldedTransaction>, HashOf<InjectedTransaction>)>,
        /// Transactions that could not be unshielded.
        not_unshielded: Vec<PurgedTransaction>,
    },

    /// Decryption shares for shielded transaction in a concrete malachite block.
    DecryptionShares {
        mb_hash: H256,
        shares: Vec<ShieldedTxDecryptionShare>,
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
            Self::UnshieldingOutput {
                mb_hash,
                unshielded_hash_mapping,
                not_unshielded,
            } => {
                write!(
                    f,
                    "UnshieldingOutput(mb_hash: {mb_hash}, unshielded_len: {}, not_unshielded_len: {})",
                    unshielded_hash_mapping.len(),
                    not_unshielded.len()
                )
            }
            Self::DecryptionShares { mb_hash, shares } => {
                write!(
                    f,
                    "DecryptionShares(mb_hash: {mb_hash}, shares_len: {})",
                    shares.len()
                )
            }
        }
    }
}

// Static check: the public types are stable.
#[cfg(test)]
#[allow(dead_code)]
fn _api_shape(
    _ev: MalachiteEvent,
    _ops: Operations,
    _cert: CommitCertificate,
    _cfg: MalachiteConfig,
    _tx: ethexe_common::injected::Transaction,
) {
}
