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
//! Ethexe-side wrapper around [`ethexe_malachite_core::MalachiteService`].
//! Stitches the [`InjectedTxMempool`], [`ethexe_malachite_core::Externalities`],
//! and the public [`MalachiteService`] facade.
//!
//! Inputs: [`Database`](ethexe_db::Database) (block storage), the latest Ethereum
//! chain head fed via `receive_new_chain_head`, and the [`Mempool`] sampled by
//! the producer.
//!
//! Outputs (`Stream<Item = Result<MalachiteEvent>>`): `BlockProposal` fires
//! after `save_block` persists, `BlockFinalized` after `mark_block_as_finalized`.
//! Both are emitted ancestor-first.

mod config;
mod externalities;
mod mempool;
mod quarantine;
mod service;

pub use crate::{
    config::{MalachiteConfig, ValidatorEntry},
    mempool::{
        DEFAULT_POOL_CAPACITY, EmptyMempool, InjectedTxMempool, Mempool, MempoolInsertError,
        classify_insert_outcome,
    },
    service::MalachiteService,
};

/// libp2p peer-id derived from a validator secret.
pub use ethexe_malachite_core::libp2p_peer_id as malachite_libp2p_peer_id;
pub use ethexe_malachite_core::{Multiaddr, PeerId, derive_libp2p_secret};

pub use ethexe_common::malachite::{
    ProcessQueuesLimits, ProgressTasksLimits, Transaction, Transactions,
};
pub use gprimitives::H256;

/// Ethexe-shaped commit certificate; `block_hash` is the Blake2b envelope hash.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommitCertificate {
    pub height: u64,
    pub block_hash: H256,
    pub signatures: Vec<Vec<u8>>,
}

/// Output event stream of the Malachite service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MalachiteEvent {
    /// New sequencer block persisted; `block_hash` is the Blake2b envelope hash.
    BlockProposal { height: u64, block_hash: H256 },

    /// BFT-committed block; `globals.latest_finalized_mb_hash` now points at it.
    BlockFinalized {
        cert: CommitCertificate,
        height: u64,
        block_hash: H256,
    },
}

impl std::fmt::Display for MalachiteEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlockProposal { height, block_hash } => {
                write!(
                    f,
                    "BlockProposal(height: {height}, block_hash: {block_hash})"
                )
            }
            Self::BlockFinalized {
                cert,
                height,
                block_hash,
            } => write!(
                f,
                "BlockFinalized(height: {}, block_hash: {}, sigs: {})",
                height,
                block_hash,
                cert.signatures.len()
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
