// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use ethexe_common::{SimpleBlockData, injected::PurgedTransaction};
use gprimitives::H256;
use tokio::sync::{Notify, RwLock};

/// Ethereum chain-head register shared between [`crate::MalachiteService`]
/// (writer) and the externalities (reader).
///
/// Invariant: only the service event loop writes these fields, and no guard
/// is ever held across an `.await` — keep it that way to stay deadlock-free.
pub struct ChainHead {
    /// Latest observed EB.
    pub latest: RwLock<SimpleBlockData>,
    /// Latest fully synced EB — reference point for quarantine and tx checks.
    pub latest_synced: RwLock<SimpleBlockData>,
    /// Wakes the producer when a new EB is synced.
    pub notify: Notify,
}

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
