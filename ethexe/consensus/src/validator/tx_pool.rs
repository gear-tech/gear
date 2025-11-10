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
    inner: HashSet<HashOf<InjectedTransaction>>,
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
        tracing::info!(tx = ?tx.data().hash(), "handle new injected tx");
        if self.inner.insert(tx.data().hash()) {
            // Write tx in database only if its not already contains in pool.
            self.db.set_injected_transaction(tx);
        }
    }

    /// Returns the injected transactions that are valid and can be included to announce.
    pub fn collect_txs_for(
        &self,
        block_hash: H256,
        parent_announce: HashOf<Announce>,
    ) -> Result<Vec<SignedInjectedTransaction>> {
        tracing::info!(block = ?block_hash, "start collecting injected transactions");

        let included_txs = self.db.announce_recent_txs(parent_announce);

        let mut txs_for_block = vec![];

        for tx_hash in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                continue;
            };

            // Skip transaction if its not valid
            match self.check_validity_at(&tx, block_hash) {
                Ok(true) if !included_txs.contains(tx_hash) => {
                    txs_for_block.push(tx);
                }
                _ => continue,
            }
        }

        Ok(txs_for_block)
    }

    // TODO #4808: branch check must be until genesis block
    /// Checks if the transaction is still valid at the given block.
    /// Checking windows is in `transaction_height..transaction_height + VALIDITY_WINDOW`
    ///
    /// # Returns
    /// - `true` if the transaction is still valid at the given block
    /// - `false` otherwise
    pub fn check_validity_at(
        &self,
        tx: &SignedInjectedTransaction,
        block_hash: H256,
    ) -> Result<bool> {
        let transaction_block_hash = tx.data().reference_block;
        let transaction_height = self
            .db
            .block_header(transaction_block_hash)
            .ok_or_else(|| {
                anyhow!("Block header not found for reference block {transaction_block_hash}")
            })?
            .height;

        let block_height = self
            .db
            .block_header(block_hash)
            .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
            .height;

        if transaction_height > block_height
            || transaction_height + VALIDITY_WINDOW as u32 <= block_height
        {
            return Ok(false);
        }

        // Check transaction inclusion in the block branch.
        let mut block_hash = block_hash;
        for _ in 0..VALIDITY_WINDOW {
            if block_hash == transaction_block_hash {
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

        let tx = mock_tx(blocks[0].hash);
        for block in blocks.iter().take(VALIDITY_WINDOW as usize) {
            assert!(tx_pool.check_validity_at(&tx, block.hash).unwrap());
        }
        assert!(
            !tx_pool
                .check_validity_at(&tx, blocks[(VALIDITY_WINDOW + 1).into()].hash)
                .unwrap()
        );

        let tx = mock_tx(blocks[10].hash);
        assert!(!tx_pool.check_validity_at(&tx, blocks[5].hash).unwrap());
        assert!(!tx_pool.check_validity_at(&tx, blocks[9].hash).unwrap());
        for block in blocks.iter().take((VALIDITY_WINDOW + 10) as usize).skip(10) {
            assert!(tx_pool.check_validity_at(&tx, block.hash).unwrap());
        }
        assert!(
            !tx_pool
                .check_validity_at(&tx, blocks[(VALIDITY_WINDOW * 2).into()].hash)
                .unwrap()
        );
    }
}
