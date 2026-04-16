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
    Announce, HashOf, MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE, SimpleBlockData,
    db::{
        AnnounceStorageRO, CodesStorageRO, GlobalsStorageRO, InjectedStorageRW, OnChainStorageRO,
    },
    injected::{InjectedTransaction, SignedInjectedTransaction},
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gprimitives::H256;
use parity_scale_codec::Encode;
use std::collections::HashSet;

/// Maximum total size of injected transactions per announce.
/// Currently set to 127 KB.
pub const MAX_INJECTED_TRANSACTIONS_SIZE_PER_ANNOUNCE: usize = 127 * 1024;

/// [`InjectedTxPool`] is a local pool of injected transactions, which validator can include in announces.
#[derive(Clone)]
pub(crate) struct InjectedTxPool<DB = Database> {
    /// HashSet of (reference_block, injected_tx_hash).
    inner: HashSet<(H256, HashOf<InjectedTransaction>)>,
    db: DB,
}

impl<DB> InjectedTxPool<DB>
where
    DB: InjectedStorageRW
        + GlobalsStorageRO
        + OnChainStorageRO
        + AnnounceStorageRO
        + CodesStorageRO
        + Storage
        + Clone,
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
        block: SimpleBlockData,
        parent_announce: HashOf<Announce>,
    ) -> Result<Vec<SignedInjectedTransaction>> {
        tracing::trace!(block = ?block.hash, "start collecting injected transactions");

        let tx_checker =
            TxValidityChecker::new_for_announce(self.db.clone(), block, parent_announce)?;

        let mut touched_programs = crate::utils::block_touched_programs(&self.db, block.hash)?;
        if touched_programs.len() > MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE as usize {
            tracing::error!(
                block = ?block.hash,
                "too many programs changed: {} > {}, may cause overflow in announce size",
                touched_programs.len(),
                MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE
            );
            return Ok(vec![]);
        }

        let mut selected_txs = vec![];
        let mut remove_txs = vec![];
        let mut size_counter = 0usize;

        for (reference_block, tx_hash) in self.inner.iter() {
            let Some(tx) = self.db.injected_transaction(*tx_hash) else {
                // This must not happen, as we store txs in db when adding to pool.
                anyhow::bail!("injected tx not found in db: {tx_hash}");
            };

            match tx_checker.check_tx_validity(&tx)? {
                TxValidity::Valid => {
                    // NOTE: we calculate size with signature, because tx will be sent to network with it.
                    let tx_size = tx.encoded_size();
                    if size_counter + tx_size > MAX_INJECTED_TRANSACTIONS_SIZE_PER_ANNOUNCE {
                        tracing::trace!(
                            ?tx_hash,
                            "transaction is valid, but exceeds max announce size limit, so skipping it for future announces"
                        );
                        continue;
                    }

                    let program_id = tx.data().destination;
                    if !touched_programs.contains(&program_id)
                        && touched_programs.len() >= MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE as usize
                    {
                        tracing::trace!(
                            ?tx_hash,
                            "transaction is valid, but max touched programs limit is reached, so skipping it now"
                        );
                        continue;
                    }

                    tracing::trace!(tx_hash = ?tx_hash, tx = ?tx.data(), "tx is valid, including to announce");

                    touched_programs.insert(program_id);
                    selected_txs.push(tx);
                    size_counter += tx_size;
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
                TxValidity::InsufficientBalanceForInjectedMessages => {
                    // Keep in pool, in case destination actor balance increases later.
                    tracing::trace!(
                        tx_hash = ?tx_hash,
                        tx = ?tx.data(),
                        "tx destination actor has insufficient balance for injected messages, keeping in pool"
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
    use crate::{mock::*, tx_validation::MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES};

    use super::*;
    use ethexe_common::{
        StateHashWithQueueSize,
        db::*,
        events::{BlockEvent, MirrorEvent, mirror::MessageQueueingRequestedEvent},
        mock::*,
    };
    use ethexe_runtime_common::state::{ActiveProgram, Program, ProgramState, Storage};
    use gear_core::program::MemoryInfix;
    use gprimitives::{ActorId, MessageId};
    use gsigner::secp256k1::{Secp256k1SignerExt, Signer};
    use parity_scale_codec::MaxEncodedLen;

    #[test]
    fn test_select_for_announce() {
        gear_utils::init_default_logger();

        let db = Database::memory();

        let state_hash = db.write_program_state(
            // Make not required init message by setting terminated state.
            ProgramState {
                program: Program::Terminated(ActorId::from([2; 32])),
                executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
                ..ProgramState::zero()
            },
        );
        let program_id = ActorId::from([1; 32]);

        let chain = test_block_chain(10)
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

                c.globals.latest_computed_announce_hash = c.block_top_announce_hash(8);
            })
            .setup(&db);

        let mut tx_pool = InjectedTxPool::new(db.clone());

        let signer = Signer::memory();
        let key = signer.generate().unwrap();
        let tx = test_injected_transaction(chain.blocks[9].hash, program_id);
        let tx_hash = tx.to_hash();
        let signed_tx = signer.signed_message(key, tx, None).unwrap();

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
                    test_injected_transaction(chain.blocks[9].hash, program_id)
                        .tap_mut(|tx| tx.value = 100),
                    None,
                )
                .unwrap(),
        );

        let selected_txs = tx_pool
            .select_for_announce(
                chain.blocks[10].to_simple(),
                chain.block_top_announce_hash(9),
            )
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

    #[test]
    fn validate_max_tx_size() {
        assert!(
            SignedInjectedTransaction::max_encoded_len()
                <= MAX_INJECTED_TRANSACTIONS_SIZE_PER_ANNOUNCE
        );
    }

    #[test]
    fn max_touched_programs() {
        gear_utils::init_default_logger();

        let db = Database::memory();

        let state = ProgramState {
            program: Program::Active(ActiveProgram {
                allocations_hash: HashOf::zero().into(),
                pages_hash: HashOf::zero().into(),
                memory_infix: MemoryInfix::new(0),
                initialized: true,
            }),
            executable_balance: MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES * 100,
            ..ProgramState::zero()
        };
        let state_hash = db.write_program_state(state);

        let chain = test_block_chain(10)
            .tap_mut(|chain| {
                chain.blocks[10].synced.events = (0..97)
                    .map(|i| BlockEvent::Mirror {
                        actor_id: ActorId::from(i),
                        event: MirrorEvent::MessageQueueingRequested(
                            MessageQueueingRequestedEvent {
                                id: MessageId::from(i * 1000),
                                source: ActorId::from(i * 10000),
                                payload: vec![],
                                value: 0,
                                call_reply: false,
                            },
                        ),
                    })
                    .collect();

                chain
                    .block_top_announce_mut(9)
                    .as_computed_mut()
                    .program_states = (0..140)
                    .map(|i| {
                        (
                            ActorId::from(i),
                            StateHashWithQueueSize {
                                hash: state_hash,
                                canonical_queue_size: 0,
                                injected_queue_size: 0,
                            },
                        )
                    })
                    .collect();

                chain.globals.latest_computed_announce_hash = chain.block_top_announce_hash(9);
            })
            .setup(&db);

        let mut tx_pool = InjectedTxPool::new(db.clone());
        let signer = Signer::memory();
        let key = signer.generate().unwrap();
        for i in 90..140 {
            let tx = test_injected_transaction(chain.blocks[9].hash, ActorId::from(i as u64));
            let signed_tx = signer.signed_message(key, tx, None).unwrap();
            tx_pool.handle_tx(signed_tx);
        }

        let selected_txs = tx_pool
            .select_for_announce(
                chain.blocks[10].to_simple(),
                chain.block_top_announce_hash(9),
            )
            .unwrap();

        assert_eq!(
            selected_txs.len(),
            MAX_TOUCHED_PROGRAMS_PER_ANNOUNCE as usize - 90
        );
    }
}
