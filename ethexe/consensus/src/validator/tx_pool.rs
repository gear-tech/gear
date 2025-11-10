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
    Announce, HashOf,
    db::{AnnounceStorageRO, InjectedStorageRW, LatestDataStorageRO, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction, VALIDITY_WINDOW},
};
use ethexe_db::Database;
use gprimitives::H256;
use std::collections::HashSet;

/// [`InjectedTxPool`] is a local pool of injected transactions, which validator can include in announces.
#[derive(Clone)]
pub(crate) struct InjectedTxPool<DB = Database> {
    inner: HashSet<(H256, HashOf<InjectedTransaction>)>,
    db: DB,
}

impl<DB> InjectedTxPool<DB>
where
    DB: OnChainStorageRO + InjectedStorageRW + LatestDataStorageRO + AnnounceStorageRO,
{
    pub fn new(db: DB) -> Self {
        Self {
            inner: HashSet::new(),
            db,
        }
    }

    pub fn handle_tx(&mut self, tx: SignedInjectedTransaction) {
        let tx_hash = tx.data().hash();
        let reference_block = tx.data().reference_block;
        tracing::info!(tx_hash = ?tx_hash, reference_block = ?reference_block,  "handle new injected tx");

        if self.inner.insert((reference_block, tx_hash)) {
            // Write tx in database only if its not already contains in pool.
            self.db.set_injected_transaction(tx);
        }
    }

    /// Returns the injected transactions that are valid and can be included to announce.
    pub fn collect_txs_for(
        &mut self,
        block_hash: H256,
        parent_announce: HashOf<Announce>,
    ) -> Result<Vec<SignedInjectedTransaction>> {
        tracing::info!(block = ?block_hash, "start collecting injected transactions");

        let already_included_txs = self.db.announce_recent_txs(parent_announce);
        let mut collected_txs = vec![];
        let mut outdated_txs = vec![];

        for (reference_block, tx_hash) in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                continue;
            };

            if !self.reference_block_within_validity_window(*reference_block, block_hash)? {
                outdated_txs.push((*reference_block, *tx_hash));
                continue;
            }

            if self.reference_block_on_current_branch(*reference_block, block_hash)?
                && !already_included_txs.contains(tx_hash)
            {
                collected_txs.push(tx);
            }
        }

        outdated_txs.into_iter().for_each(|key| {
            self.inner.remove(&key);
        });

        Ok(collected_txs)
    }

    fn reference_block_within_validity_window(
        &self,
        reference_block: H256,
        chain_head: H256,
    ) -> Result<bool> {
        let reference_block_height = self
            .db
            .block_header(reference_block)
            .ok_or_else(|| anyhow!("Block header not found for reference block {reference_block}"))?
            .height;

        let chain_head_height = self
            .db
            .block_header(chain_head)
            .ok_or_else(|| anyhow!("Block header not found for hash: {chain_head}"))?
            .height;

        Ok(reference_block_height <= chain_head_height
            && reference_block_height + VALIDITY_WINDOW as u32 > chain_head_height)
    }

    // TODO #4808: branch check must be until genesis block
    fn reference_block_on_current_branch(
        &self,
        reference_block: H256,
        chain_head: H256,
    ) -> Result<bool> {
        let mut block_hash = chain_head;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        Address,
        ecdsa::PrivateKey,
        mock::{BlockChain, Mock},
    };

    fn mock_tx(reference_block: H256) -> SignedInjectedTransaction {
        let tx = InjectedTransaction {
            recipient: Address::default(),
            destination: Address::default().into(),
            payload: H256::random().0.to_vec().into(),
            value: 0,
            reference_block,
            salt: H256::random().0.to_vec().into(),
        };

        SignedInjectedTransaction::create(PrivateKey::random(), tx).unwrap()
    }

    #[test]
    fn test_check_mortality_at() {
        let db = Database::memory();

        // setup database for testing
        let blocks = BlockChain::mock(100).setup(&db).blocks;

        let tx_pool = InjectedTxPool::new(db);

        let is_valid = |tx: &SignedInjectedTransaction, at_block: H256| {
            let reference_block = tx.data().reference_block;
            tx_pool
                .reference_block_within_validity_window(reference_block, at_block)
                .unwrap()
                && tx_pool
                    .reference_block_on_current_branch(reference_block, at_block)
                    .unwrap()
        };

        let tx = mock_tx(blocks[0].hash);
        for block in blocks.iter().take(VALIDITY_WINDOW as usize) {
            assert!(is_valid(&tx, block.hash));
        }
        assert!(!is_valid(&tx, blocks[(VALIDITY_WINDOW + 1).into()].hash));

        let tx = mock_tx(blocks[10].hash);
        assert!(!is_valid(&tx, blocks[5].hash));
        assert!(!is_valid(&tx, blocks[9].hash));
        for block in blocks.iter().take((VALIDITY_WINDOW + 10) as usize).skip(10) {
            assert!(is_valid(&tx, block.hash));
        }
        assert!(!is_valid(&tx, blocks[(VALIDITY_WINDOW * 2).into()].hash));
    }
}
