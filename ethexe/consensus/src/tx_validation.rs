// This file is part of Gear.
//
// Copyright (C) 2025 Gear Technologies Inc.
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

use anyhow::{Result, anyhow};
use ethexe_common::{
    Announce, HashOf, ProgramStates,
    db::{AnnounceStorageRO, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction, VALIDITY_WINDOW},
};
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use hashbrown::HashSet;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TxValidity {
    /// Transaction is valid and can be include into announce.
    Valid,
    /// Transaction was already include into one of previous [`VALIDITY_WINDOW`] announces.
    Duplicate,
    /// Transaction is outdated and should be remove from pool.
    Outdated,
    /// Transaction's reference block not on current branch.
    /// Keep tx in pool in case of reorg.
    NotOnCurrentBranch,
    /// Transaction's destination [`gprimitives::ActorId`] not found.
    UnknownDestination,
    /// Transaction's destination [`gprimitives::ActorId`] not initialize.
    UninitializedDestination,
}

pub struct TxValidityChecker<DB> {
    db: DB,
    chain_head: H256,
    recent_included_txs: HashSet<HashOf<InjectedTransaction>>,
    latest_states: ProgramStates,
}

impl<DB: OnChainStorageRO + AnnounceStorageRO + Storage> TxValidityChecker<DB> {
    pub fn new_for_announce(db: DB, chain_head: H256, announce: HashOf<Announce>) -> Result<Self> {
        Ok(Self {
            recent_included_txs: Self::collect_recent_included_txs(&db, announce)?,
            latest_states: db.announce_program_states(announce).unwrap_or_default(),
            db,
            chain_head,
        })
    }

    /// Determine [`TxValidity`] status for injected transaction, based on current:
    /// - `chain_head` - Ethereum chain header
    /// - `latest_included_transactions` - see [`Self::collect_recent_included_txs`].
    pub fn check_tx_validity(&self, tx: &SignedInjectedTransaction) -> Result<TxValidity> {
        let reference_block = tx.data().reference_block;

        if !self.is_reference_block_within_validity_window(reference_block)? {
            return Ok(TxValidity::Outdated);
        }

        if !self.is_reference_block_on_current_branch(reference_block)? {
            return Ok(TxValidity::NotOnCurrentBranch);
        }

        if self.recent_included_txs.contains(&tx.data().to_hash()) {
            return Ok(TxValidity::Duplicate);
        }

        let Some(destination_state_hash) = self.latest_states.get(&tx.data().destination) else {
            return Ok(TxValidity::UnknownDestination);
        };

        let Some(state) = self.db.program_state(destination_state_hash.hash) else {
            // TODO kuzmin: consider in future add a new `TxValidity` veriant or return TxValidity::UnknownDestination.
            unreachable!("program state must be found by its valid hash.")
        };

        if state.requires_init_message() {
            return Ok(TxValidity::UninitializedDestination);
        }

        Ok(TxValidity::Valid)
    }

    fn is_reference_block_within_validity_window(&self, reference_block: H256) -> Result<bool> {
        let reference_block_height = self
            .db
            .block_header(reference_block)
            .ok_or_else(|| anyhow!("Block header not found for reference block {reference_block}"))?
            .height;

        let chain_head_height = self
            .db
            .block_header(self.chain_head)
            .ok_or_else(|| anyhow!("Block header not found for hash: {}", self.chain_head))?
            .height;

        Ok(reference_block_height <= chain_head_height
            && reference_block_height + VALIDITY_WINDOW as u32 > chain_head_height)
    }

    // TODO #4808: branch check must be until genesis block
    fn is_reference_block_on_current_branch(&self, reference_block: H256) -> Result<bool> {
        let mut block_hash = self.chain_head;
        for _ in 0..VALIDITY_WINDOW {
            if block_hash == reference_block {
                return Ok(true);
            }

            block_hash = self
                .db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }

    /// Collects hashes of [`InjectedTransaction`] from recent announce within [`VALIDITY_WINDOW`].
    pub fn collect_recent_included_txs(
        db: &DB,
        announce: HashOf<Announce>,
    ) -> Result<HashSet<HashOf<InjectedTransaction>>> {
        let mut txs = HashSet::new();

        let mut announce_hash = announce;
        for _ in 0..VALIDITY_WINDOW {
            let Some(announce) = db.announce(announce_hash) else {
                // Reach genesis_announce - correct case.
                if announce_hash == HashOf::zero() {
                    break;
                }

                // TODO: #4969 temporary hack ignoring this error for fast_sync test.
                // Reach start announce is not correct case, because of can exists earlier announces with injected txs.
                // anyhow::bail!("Reaching start announce is not supported; decrease VALIDITY_WINDOW")
                break;
            };

            announce_hash = announce.parent;

            txs.extend(
                announce
                    .injected_transactions
                    .into_iter()
                    .map(|tx| tx.data().to_hash()),
            );
        }

        Ok(txs)
    }
}
