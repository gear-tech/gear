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

use crate::tx_validation::{TxValidity, TxValidityChecker};
use anyhow::Result;
use ethexe_common::{
    Announce, HashOf,
    db::{AnnounceStorageRO, CodesStorageRO, InjectedStorageRW, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction},
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use std::collections::HashSet;

/// [`InjectedTxPool`] is a local pool of injected transactions, which validator can include in announces.
#[derive(Clone)]
pub(crate) struct InjectedTxPool<DB = Database> {
    /// HashSet of (reference_block, injected_tx_hash).
    inner: HashSet<(H256, HashOf<InjectedTransaction>)>,
    db: DB,
}

impl<DB> InjectedTxPool<DB>
where
    DB: OnChainStorageRO + InjectedStorageRW + AnnounceStorageRO + CodesStorageRO + Storage + Clone,
{
    pub fn new(db: DB) -> Self {
        Self {
            inner: HashSet::new(),
            db,
        }
    }

    pub fn handle_tx(&mut self, tx: SignedInjectedTransaction) {
        let tx_hash = tx.data().to_hash();
        let reference_block = tx.data().reference_block;
        tracing::trace!(tx_hash = ?tx_hash, reference_block = ?reference_block,  "handle new injected tx");

        if self.inner.insert((reference_block, tx_hash)) {
            // Write tx in database only if its not already contains in pool.
            self.db.set_injected_transaction(tx);
        }
    }

    /// Returns the injected transactions that are valid and can be included to announce.
    pub fn select_for_announce(
        &mut self,
        block_hash: H256,
        parent_announce: HashOf<Announce>,
    ) -> Result<Vec<SignedInjectedTransaction>> {
        tracing::trace!(block = ?block_hash, "start collecting injected transactions");

        let tx_checker =
            TxValidityChecker::new_for_announce(self.db.clone(), block_hash, parent_announce)?;

        let mut selected_txs = vec![];
        let mut outdated_txs = vec![];

        for (reference_block, tx_hash) in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                continue;
            };

            match tx_checker.check_tx_validity(&tx)? {
                TxValidity::Valid => selected_txs.push(tx),
                TxValidity::Duplicate => {
                    // TODO kuzmindev: send result to submitter, that tx was already included.
                }
                TxValidity::UnknownDestination => {
                    // TODO kuzmindev: also send to submitter result, that tx `destination` field is invalid.
                }
                TxValidity::NotOnCurrentBranch => {
                    tracing::trace!(tx_hash = ?tx_hash, "tx on different branch, keeping in pool");
                }
                TxValidity::Outdated => outdated_txs.push((*reference_block, *tx_hash)),
                TxValidity::UninitializedDestination => {
                    tracing::trace!(
                        tx_hash = ?tx_hash,
                        "tx send to uninitialized actor, keeping in pool, because of in next blocks it can be"
                    );
                }
            }
        }

        outdated_txs.into_iter().for_each(|key| {
            self.inner.remove(&key);
        });

        Ok(selected_txs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        SimpleBlockData, StateHashWithQueueSize,
        db::{AnnounceStorageRW, OnChainStorageRW},
        ecdsa::PrivateKey,
        injected::VALIDITY_WINDOW,
        mock::{BlockChain, Mock},
    };
    use gprimitives::ActorId;
    use std::collections::BTreeMap;

    fn mock_tx(reference_block: H256) -> SignedInjectedTransaction {
        let mut tx = InjectedTransaction::mock(());
        tx.reference_block = reference_block;
        tx.destination = ActorId::zero();

        SignedInjectedTransaction::create(&PrivateKey::random(), tx).unwrap()
    }

    fn setup_announce(db: &Database, txs: Vec<SignedInjectedTransaction>) -> HashOf<Announce> {
        let mut announce = Announce::mock(());
        announce.parent = HashOf::zero();
        announce.injected_transactions = txs;
        let announce_hash = db.set_announce(announce);

        let state = StateHashWithQueueSize {
            canonical_queue_size: 0,
            injected_queue_size: 0,
            hash: H256::zero(),
        };
        db.set_announce_program_states(announce_hash, BTreeMap::from([(ActorId::zero(), state)]));

        announce_hash
    }

    #[test]
    fn test_check_tx_validity() {
        let db = Database::memory();
        let blocks = BlockChain::mock(100).setup(&db).blocks;

        let announce_hash = setup_announce(&db, vec![]);

        let chain_head = blocks[VALIDITY_WINDOW as usize].hash;
        let tx_checker =
            TxValidityChecker::new_for_announce(db, chain_head, announce_hash).unwrap();

        for block in blocks.iter().skip(1).take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Valid,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }

    #[test]
    fn test_check_tx_duplicate() {
        let db = Database::memory();
        let blocks = BlockChain::mock(100).setup(&db).blocks;

        let tx = mock_tx(blocks[5].hash);
        let announce_hash = setup_announce(&db, vec![tx.clone()]);

        let tx_checker =
            TxValidityChecker::new_for_announce(db, blocks[9].hash, announce_hash).unwrap();

        assert_eq!(
            TxValidity::Duplicate,
            tx_checker.check_tx_validity(&tx).unwrap()
        );
    }

    #[test]
    fn test_check_tx_outdated() {
        let db = Database::memory();
        let blocks = BlockChain::mock(100).setup(&db).blocks;

        let announce_hash = setup_announce(&db, vec![]);

        let chain_head = blocks[(VALIDITY_WINDOW * 2) as usize].hash;
        let tx_checker =
            TxValidityChecker::new_for_announce(db, chain_head, announce_hash).unwrap();

        for block in blocks.iter().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Outdated,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }

    #[test]
    fn test_check_tx_not_on_current_branch() {
        let db = Database::memory();
        let blocks = BlockChain::mock(35).setup(&db).blocks;

        let mut blocks_branch2 = vec![];

        let mut parent = blocks[10].hash;
        blocks.iter().skip(9).for_each(|block| {
            let mut header = block.to_simple().header;
            header.parent_hash = parent;

            let hash = H256::random();
            db.set_block_header(hash, header);
            blocks_branch2.push(SimpleBlockData { hash, header });
            parent = hash;
        });

        let announce_hash = setup_announce(&db, vec![]);

        let tx_checker =
            TxValidityChecker::new_for_announce(db, blocks[35].hash, announce_hash).unwrap();

        for block in blocks_branch2.iter() {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::NotOnCurrentBranch,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }

        for block in blocks.iter().rev().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Valid,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }
}
