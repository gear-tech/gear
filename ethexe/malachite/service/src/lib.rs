// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Malachite
//!
//! Ethexe-side application glue around `ethexe-malachite-core`, the generic
//! Malachite BFT / Tendermint-style consensus engine. This crate does not
//! implement BFT voting, gossip, peer discovery, or persistence — those all live in
//! `ethexe-malachite-core`. It provides the public [`MalachiteService`] facade,
//! the producer-side [`Mempool`] abstraction with its injected-transaction
//! implementations, per-transaction validity checking, and event translation from
//! engine callbacks into [`MalachiteEvent`].
//!
//! ## Responsibilities
//!
//! - Expose [`MalachiteService`] as a `Stream<Item = Result<MalachiteEvent>>` that
//!   `ethexe-service` selects on.
//! - Bridge the consensus engine to ethexe storage via `EthexeExternalities`
//!   (private), which reads block data from an [`ethexe_db::Database`].
//! - Maintain the producer-side [`Mempool`] of [`SignedInjectedTransaction`]s;
//!   [`InjectedTxMempool`] is the real implementation, [`EmptyMempool`] is the
//!   no-op used by full nodes.
//! - Validate each injected transaction against the current Malachite-block world
//!   via [`TxValidityChecker`], producing a [`TxValidity`] verdict used both for
//!   mempool GC and for producer/validator MB acceptance.
//! - Translate engine finalization callbacks into [`MalachiteEvent::BlockProposal`]
//!   and [`MalachiteEvent::BlockFinalized`], both emitted ancestor-first.
//!
//! ## Role in the Stack
//!
//! ```text
//! ethexe-observer  ──→  (chain head)  ──→  MalachiteService
//! ethexe-common        (injected txs) ──→  InjectedTxMempool
//!                                               │
//!                             ethexe-malachite-core (BFT engine)
//!                                               │
//!                        Stream<MalachiteEvent> ↓
//!                               ethexe-service / ethexe-cli
//! ```
//!
//! `ethexe-observer` feeds the latest Ethereum chain head into the service via
//! `receive_new_chain_head`. `ethexe-service` is the sole consumer of the output
//! stream and constructs the service at startup; `ethexe-cli` is the binary entry
//! point that drives `ethexe-service`.
//!
//! ## Entry Points / Public API
//!
//! | Item | Kind | Purpose |
//! |---|---|---|
//! | [`MalachiteService`] | struct | Public facade; `Stream` + driver methods |
//! | [`MalachiteEvent`] | enum | Output event: proposal, finalization, purged txs |
//! | [`CommitCertificate`] | struct | BFT commit proof attached to `BlockFinalized` |
//! | [`MalachiteConfig`] | struct | Service configuration (validators, listen address, WAL dir, gas allowance) |
//! | [`ValidatorEntry`] | struct | Single entry in the validator set |
//! | [`Mempool`] | trait | Producer-side injected-tx source |
//! | [`InjectedTxMempool`] | struct | Real mempool implementation |
//! | [`EmptyMempool`] | struct | No-op mempool for full nodes |
//! | [`TxValidityChecker`] | struct | Per-tx validity against the MB world |
//! | [`TxValidity`] | enum | Validity verdict: `Valid`, `Duplicate`, `Outdated`, … |
//!
//! Driver methods on [`MalachiteService`]: `receive_injected_transaction`,
//! `receive_new_chain_head`, `receive_eb_prepared`, `shutdown`.
//!
//! ## Key Types
//!
//! **[`MalachiteService`]** — constructed with
//! `MalachiteService::new(config, db, signer, validator_pub_key, mempool)` (async).
//! `validator_pub_key: Some(_)` starts the node as a `Validator` (the key must
//! appear in `config.validators`); `None` starts it as a `FullNode` that
//! participates in gossip and sync only. `Drop` is best-effort; call
//! `shutdown().await` before an immediate restart to ensure RocksDB locks and
//! sockets release.
//!
//! **[`MalachiteEvent`]** — three variants emitted by the stream:
//! - `BlockProposal { height, mb_hash }` — a new sequencer block has been
//!   persisted.
//! - `BlockFinalized { cert, height, mb_hash }` — the block has reached BFT
//!   commit; `cert` carries the aggregated signatures.
//! - `PurgedTransactions { eb_hash, transactions }` — injected transactions that
//!   were removed from the mempool after finalization.
//!
//! **[`Mempool`]** — `fetch` is non-destructive; `forget` runs after MB
//! finalization and deduplicates within `VALIDITY_WINDOW`.
//!
//! **[`TxValidity`]** — non-`Valid` variants distinguish "drop from pool" (e.g.
//! `Duplicate`, `Outdated`) from "keep, may become valid on reorg / later state
//! change" (e.g. `NotOnCurrentBranch`, `UnknownDestination`). The validator
//! rejects the entire MB if any transaction is not `Valid`.
//!
//! ## Invariants
//!
//! - `BlockProposal` is always emitted before the corresponding `BlockFinalized`
//!   for the same height; both series are emitted ancestor-first.
//! - [`MalachiteService::new`] returns `Err` if `config.validators` is empty or
//!   the local node's public key is absent from the list.
//! - Voting power is taken at face value; Tendermint's quorum threshold is
//!   `> 2/3` of total voting power across the validator list.
//! - Peer discovery is disabled; every `persistent_peers` multiaddr must include
//!   the `/p2p/<peer_id>` suffix, and every validator must be listed.

mod config;
mod externalities;
mod mempool;
mod quarantine;
mod service;
mod tx_validity;

pub use crate::{
    config::{MalachiteConfig, ValidatorEntry},
    mempool::{
        DEFAULT_POOL_CAPACITY, EmptyMempool, InjectedTxMempool, Mempool, MempoolInsertError,
        classify_insert_outcome,
    },
    service::MalachiteService,
    tx_validity::{MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES, TxValidity, TxValidityChecker},
};
use ethexe_common::injected::PurgedTransaction;
pub use ethexe_common::{
    injected::SignedInjectedTransaction,
    malachite::{ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions},
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

// Static check: the public types are stable.
#[cfg(test)]
#[allow(dead_code)]
fn _api_shape(
    _ev: MalachiteEvent,
    _block: Transactions,
    _cert: CommitCertificate,
    _mp: EmptyMempool,
    _cfg: MalachiteConfig,
    _tx: ethexe_common::injected::SignedInjectedTransaction,
) {
}
