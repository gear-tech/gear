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

use crate::tx_validation::TxValidityChecker;
use anyhow::Result;
use ethexe_common::{
    Announce, HashOf,
    db::{AnnounceStorageRO, CodesStorageRO, InjectedStorageRW, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction, TxRejection, TxValidity},
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use std::collections::HashSet;

/// [`InjectedTxPool`] is a local pool of injected transactions, which validator can include in announces.
#[derive(Clone)]
pub(crate) struct InjectedTxPool<DB = Database> {
    /// HashSet of injected_tx_hash.
    inner: HashSet<HashOf<InjectedTransaction>>,
    db: DB,
}

#[derive(Debug, Clone, Default)]
pub struct TxPoolOutput {
    /// Selected transactions to be included in announce.
    pub selected_txs: Vec<SignedInjectedTransaction>,
    /// Invalid transactions reasons.
    pub rejected_txs: Vec<TxRejection>,
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
        tracing::trace!(tx_hash = ?tx_hash, reference_block = ?tx.data().reference_block,  "handle new injected tx");

        if self.inner.insert(tx_hash) {
            // Write tx in database only if its not already contains in pool.
            self.db.set_injected_transaction(tx);
        }
    }

    /// Returns the injected transactions that are valid and can be included to announce.
    pub fn select_for_announce(
        &mut self,
        block_hash: H256,
        parent_announce: HashOf<Announce>,
    ) -> Result<TxPoolOutput> {
        tracing::trace!(block = ?block_hash, "start collecting injected transactions");

        let tx_checker =
            TxValidityChecker::new_for_announce(self.db.clone(), block_hash, parent_announce)?;

        let mut output = TxPoolOutput::default();
        let mut to_remove = Vec::new();

        for tx_hash in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                // This must not happen, as we store txs in db when adding to pool.
                anyhow::bail!("injected tx not found in db: {tx_hash}");
            };

            match tx_checker.check_tx_validity(&tx)? {
                TxValidity::Valid => {
                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is valid, including to announce");
                    output.selected_txs.push(tx)
                }
                TxValidity::Intermediate(status) => {
                    tracing::trace!(tx_hash = ?tx_hash, state = %status, "tx is in intermediate state, keeping in pool")
                }
                TxValidity::Invalid(reason) => {
                    tracing::trace!(tx_hash = ?tx_hash, invalidity_reason = %reason, "tx is invalid, removing from pool");
                    output.rejected_txs.push(TxRejection {
                        tx_hash: *tx_hash,
                        reason,
                    });
                    to_remove.push(*tx_hash)
                }
            }
        }

        // Remove invalid transactions from pool.
        to_remove.into_iter().for_each(|tx_hash| {
            self.inner.remove(&tx_hash);
        });

        Ok(output)
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
        let signed_tx = signer.signed_data(key, tx).unwrap();

        tx_pool.handle_tx(signed_tx.clone());
        assert!(
            db.injected_transaction(tx_hash).is_some(),
            "tx should be stored in db"
        );

        let selected_txs = tx_pool
            .select_for_announce(chain.blocks[10].hash, chain.block_top_announce_hash(9))
            .unwrap();
        assert_eq!(
            selected_txs,
            vec![signed_tx],
            "tx should be selected for announce"
        );
    }
}
