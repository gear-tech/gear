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
    injected::{InjectedTransaction, SignedInjectedTransaction, TxRemovalInfo, TxValidity},
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use std::collections::HashSet;

/// [`TransactionPool`] is a local pool of [`InjectedTransaction`]s, which validator can include in announces.
#[derive(Clone)]
pub(crate) struct TransactionPool<DB = Database> {
    inner: HashSet<HashOf<InjectedTransaction>>,
    db: DB,
}

#[derive(Debug, Clone, Default)]
pub struct TxPoolOutput {
    /// Selected transactions to be included in announce.
    pub selected_txs: Vec<SignedInjectedTransaction>,
    /// Invalid transactions reasons.
    pub removed_txs: Vec<TxRemovalInfo>,
}

/// This error returned when user trying to add the same transaction twice to the pool.
#[derive(Debug, Clone, thiserror::Error)]
#[error("Injected transaction with hash {0} already exists in the pool")]
pub struct TxDuplicateError(pub HashOf<InjectedTransaction>);

impl<DB> TransactionPool<DB>
where
    DB: OnChainStorageRO + InjectedStorageRW + AnnounceStorageRO + CodesStorageRO + Storage + Clone,
{
    pub fn new(db: DB) -> Self {
        Self {
            inner: HashSet::new(),
            db,
        }
    }

    /// Adds new injected transaction to the pool.
    /// Returns an error if transaction is already present in the pool.
    pub fn add_transaction(
        &mut self,
        tx: SignedInjectedTransaction,
    ) -> Result<(), TxDuplicateError> {
        let tx_hash = tx.data().to_hash();
        tracing::trace!(?tx_hash, reference_block = ?tx.data().reference_block, "tx pool received new injected transaction");

        if self.inner.insert(tx_hash) {
            // Write tx in database only if its not already contains in pool.
            self.db.set_injected_transaction(tx);
            return Ok(());
        }
        Err(TxDuplicateError(tx_hash))
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
                    output.removed_txs.push(TxRemovalInfo {
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

        let mut tx_pool = TransactionPool::new(db.clone());

        let signer = Signer::memory();
        let key = signer.generate_key().unwrap();
        let tx = InjectedTransaction {
            reference_block: chain.blocks[9].hash,
            destination: program_id,
            ..InjectedTransaction::mock(())
        };
        let tx_hash = tx.to_hash();
        let signed_tx = signer.signed_message(key, tx).unwrap();

        tx_pool
            .add_transaction(signed_tx.clone())
            .expect("transaction is not duplicate");
        assert!(
            db.injected_transaction(tx_hash).is_some(),
            "tx should be stored in db"
        );

        let output = tx_pool
            .select_for_announce(chain.blocks[10].hash, chain.block_top_announce_hash(9))
            .unwrap();
        assert!(output.removed_txs.is_empty(), "no tx should be removed");
        assert_eq!(
            output.selected_txs,
            vec![signed_tx],
            "tx should be selected for announce"
        );
    }
}
