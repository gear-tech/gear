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
        let mut remove_txs = vec![];

        for (reference_block, tx_hash) in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                // This must not happen, as we store txs in db when adding to pool.
                anyhow::bail!("injected tx not found in db: {tx_hash}");
            };

            match tx_checker.check_tx_validity(&tx)? {
                TxValidity::Valid => {
                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is valid, including to announce");
                    selected_txs.push(tx)
                }
                TxValidity::Duplicate => {
                    // Keep in pool, in case of reorg it can be valid again.
                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is already included in chain, keeping in pool");
                }
                TxValidity::UnknownDestination => {
                    // Keep in pool, in case reorg destination may become known.
                    tracing::trace!(
                        tx_hash = ?tx_hash,
                        tx = ?tx.data(),
                        "tx destination actor is unknown, keeping in pool"
                    );
                }
                TxValidity::NotOnCurrentBranch => {
                    // Keep in pool, in case of reorg it can be valid again.
                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is on different branch, keeping in pool");
                }
                TxValidity::Outdated => {
                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is outdated, removing from pool");
                    remove_txs.push((*reference_block, *tx_hash))
                }
                TxValidity::UninitializedDestination => {
                    // Keep in pool, in case destination actor gets initialized later.
                    tracing::trace!(
                        tx_hash = ?tx_hash,
                        tx = ?tx.data(),
                        "tx sent to uninitialized actor, keeping in pool"
                    );
                }
                TxValidity::NonZeroValue => {
                    tracing::trace!(
                        tx_hash = ?tx_hash,
                        tx = ?tx.data(),
                        "tx has non-zero value, removing from pool"
                    );
                    remove_txs.push((*reference_block, *tx_hash))
                }
            }
        }

        remove_txs.into_iter().for_each(|key| {
            self.inner.remove(&key);
        });

        Ok(selected_txs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{StateHashWithQueueSize, db::*, mock::*};
    use ethexe_runtime_common::state::{Program, ProgramState, Storage};
    use ethexe_signer::Signer;
    use gprimitives::ActorId;

    #[test]
    fn test_select_for_announce() {
        let db = Database::memory();

        let state_hash = db.write_program_state(
            // Make not required init message by setting terminated state.
            ProgramState::zero()
                .tap_mut(|s| s.program = Program::Terminated(ActorId::from([2; 32]))),
        );
        let program_id = ActorId::from([1; 32]);

        let chain = BlockChain::mock(10)
            .tap_mut(|c| {
                // set 2 last announces as not computed
                c.block_top_announce_mut(10).computed = None;
                c.block_top_announce_mut(9).computed = None;

                // append program to the announce at height 8
                c.block_top_announce_mut(8)
                    .as_computed_mut()
                    .program_states
                    .insert(
                        program_id,
                        StateHashWithQueueSize {
                            hash: state_hash,
                            canonical_queue_size: 0,
                            injected_queue_size: 0,
                        },
                    );
            })
            .setup(&db);

        let mut tx_pool = InjectedTxPool::new(db.clone());

        let signer = Signer::memory();
        let key = signer.generate_key().unwrap();
        let tx = InjectedTransaction {
            reference_block: chain.blocks[9].hash,
            destination: program_id,
            ..InjectedTransaction::mock(())
        };
        let tx_hash = tx.to_hash();
        let signed_tx = signer.signed_message(key, tx).unwrap();

        tx_pool.handle_tx(signed_tx.clone());
        assert!(
            db.injected_transaction(tx_hash).is_some(),
            "tx should be stored in db"
        );

        // Append another tx with non-zero value, should be removed during selection.
        tx_pool.handle_tx(
            signer
                .signed_message(
                    key,
                    InjectedTransaction {
                        reference_block: chain.blocks[9].hash,
                        value: 100,
                        destination: program_id,
                        ..InjectedTransaction::mock(())
                    },
                )
                .unwrap(),
        );

        let selected_txs = tx_pool
            .select_for_announce(chain.blocks[10].hash, chain.block_top_announce_hash(9))
            .unwrap();
        assert_eq!(
            selected_txs,
            vec![signed_tx],
            "tx should be selected for announce"
        );
        assert_eq!(
            tx_pool.inner.len(),
            1,
            "only one valid tx should remain in pool"
        );
    }
}
