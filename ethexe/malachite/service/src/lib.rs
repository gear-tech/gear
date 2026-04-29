// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! # Ethexe Malachite
//!
//! Ethexe-side wrapper around the application-agnostic
//! [`ethexe_malachite_core::MalachiteService`]. Stitches together:
//!
//! - the ethexe [`InjectedTxMempool`] (pulls user transactions into
//!   each producer's [`Transactions`] payload),
//! - [`EthexeExternalities`] — the [`ethexe_malachite_core::Externalities`]
//!   implementation that builds new sequencer blocks, validates
//!   incoming proposals against ethexe's quarantine rules, and
//!   persists every saved/finalized MB into [`ethexe_db::Database`],
//! - [`MalachiteService`] — the public façade exposing the same API
//!   shape the rest of ethexe consumed before the migration to
//!   `ethexe-malachite-core`.
//!
//! ## Inputs
//! - [`Database`](ethexe_db::Database) — block storage and the
//!   parent-link source for the canonical-quarantine walks.
//! - [`MalachiteService::receive_new_chain_head`] — the latest
//!   Ethereum block from the observer event stream. Only the newest
//!   value is retained; it serves as the reference point for the
//!   producer's quarantine anchor.
//! - [`Mempool`] — sampled by the producer when assembling the next
//!   sequencer block; finalized injected transactions are flushed
//!   from it via [`Mempool::forget`] from the externalities.
//!
//! ## Outputs (`Stream<Item = Result<MalachiteEvent>>`)
//! - [`MalachiteEvent::BlockProposal`] — fires only after
//!   [`ethexe_malachite_core::Externalities::save_block`] has persisted the MB
//!   into the ethexe DB. ethexe-malachite-core's strict ordering guarantees that
//!   `save_block` runs ancestor-first, so the heights surfaced here
//!   are non-decreasing.
//! - [`MalachiteEvent::BlockFinalized`] — fires only after
//!   [`ethexe_malachite_core::Externalities::mark_block_as_finalized`] has run for
//!   `cert.block_hash`; same ancestor-first ordering.

mod config;
mod externalities;
mod mempool;
mod quarantine;
mod service;

pub use crate::{
    config::{MalachiteConfig, ValidatorEntry},
    mempool::{DEFAULT_POOL_CAPACITY, EmptyMempool, InjectedTxMempool, Mempool},
    service::MalachiteService,
};

/// libp2p peer id of the Malachite swarm associated with a validator
/// secret — re-exported under the historic `malachite_libp2p_peer_id`
/// name so existing callers (cli `malachite peer-id`, integration
/// tests) keep compiling.
pub use ethexe_malachite_core::libp2p_peer_id as malachite_libp2p_peer_id;
pub use ethexe_malachite_core::{Multiaddr, PeerId, derive_libp2p_secret};

pub use ethexe_common::mb::{
    ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions,
};
pub use gprimitives::H256;

/// Commit certificate — ethexe-shaped, mirrors the
/// [`ethexe_malachite_core::CommitCertificate`] payload. `block_hash`
/// is the `ethexe_malachite_core::Block` envelope hash (Blake2b),
/// which is the same key downstream ethexe consumers index MB state
/// by in the DB.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitCertificate {
    pub height: u64,
    pub block_hash: H256,
    pub signatures: Vec<Vec<u8>>,
}

/// Output event stream of the Malachite service.
///
/// `height` is the Malachite sequencer height at which the block was
/// produced or finalized — reported here (rather than embedded
/// inside the payload) because [`Transactions`] is just an ordered
/// list with no self-referential height field.
#[derive(Debug, Clone)]
pub enum MalachiteEvent {
    /// A new sequencer block has been persisted. Fires once
    /// [`ethexe_malachite_core::Externalities::save_block`] returns
    /// Ok, after the ethexe DB (`mb_compact_block`, `mb_meta`, CAS
    /// transactions blob) has been updated.
    ///
    /// `block_hash` is the consensus envelope hash (Blake2b over
    /// `ethexe_malachite_core::Block`) — the DB key for the matching
    /// [`ethexe_common::db::CompactBlock`] and the `mb_meta` row.
    BlockProposal {
        height: u64,
        block_hash: H256,
        block: Transactions,
    },

    /// A sequencer block has been committed by the BFT quorum and
    /// `globals.latest_finalized_mb_hash` has been advanced to point
    /// at it. Fires after
    /// [`ethexe_malachite_core::Externalities::mark_block_as_finalized`]
    /// returns Ok.
    BlockFinalized {
        cert: CommitCertificate,
        block: Transactions,
    },
}

impl std::fmt::Display for MalachiteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockProposal {
                height,
                block_hash,
                block,
            } => {
                write!(
                    f,
                    "BlockProposal(height: {}, block_hash: {}, txs: {})",
                    height,
                    block_hash,
                    block.len()
                )
            }
            Self::BlockFinalized { cert, block } => write!(
                f,
                "BlockFinalized(height: {}, block_hash: {}, sigs: {}, txs: {})",
                cert.height,
                cert.block_hash,
                cert.signatures.len(),
                block.len()
            ),
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
