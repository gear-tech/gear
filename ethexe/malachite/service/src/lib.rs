// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! # Ethexe Malachite
//!
//! Ethexe-side wrapper around [`ethexe_malachite_core::MalachiteService`].
//! Stitches the [`InjectedTxMempool`], [`ethexe_malachite_core::Externalities`],
//! and the public [`MalachiteService`] facade.
//!
//! Inputs: [`Database`](ethexe_db::Database) (block storage), the latest Ethereum
//! chain head fed via `receive_new_chain_head`, and the [`Mempool`] sampled by
//! the producer.
//!
//! Outputs (`Stream<Item = Result<MalachiteEvent>>`): `BlockProposal` fires
//! after `process_mb_proposal` persists, `BlockFinalized` after
//! `process_mb_finalized`. Both are emitted ancestor-first.

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
