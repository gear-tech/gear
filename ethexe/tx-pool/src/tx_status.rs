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

use anyhow::{Result, anyhow};
use ethexe_common::{
    Announce, HashOf, ProgramStates,
    db::{AnnounceStorageRO, OnChainStorageRO},
    injected::{InjectedTransaction, SignedInjectedTransaction, VALIDITY_WINDOW},
};
use ethexe_runtime_common::state::Storage;
use gprimitives::{ActorId, H256};
use hashbrown::HashSet;

/// The status of [`InjectedTransaction`] for specific announce and chain head.
#[derive(
    Debug, Clone, PartialEq, Eq, derive_more::From, derive_more::Display, derive_more::Unwrap,
)]
pub enum TransactionStatus {
    /// Transaction is valid and can be included into announce.
    Valid,
    /// Transaction is in pending status ([`PendingStatus`]).
    #[from]
    Pending(PendingStatus),
    /// Transaction is invalid.
    /// [`crate::RemovalNotification`] will be returned to the transaction sender.
    #[from]
    Invalid(InvalidityReason),
}

/// A pending status means that the transaction is invalid right now, but
/// it may become valid in the future (e.g., after a reorg).
///
/// In this status, the transaction should be kept in the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum PendingStatus {
    /// If the transaction reference block is not on the current branch, we keep it in the pool because of possible chain reorgs.
    #[display("Transaction's reference block is not on current branch")]
    NotOnCurrentBranch,
    /// If the transaction is sent to an uninitialized actor, we keep it in the pool
    /// because the actor may be initialized in a future block.
    #[display("Transaction's destination actor({_0}) is uninitialized")]
    UninitializedDestination(ActorId),
}

/// The reason why the transaction is not valid and cannot be included into announce.
///
/// Note: serialize as [`u8`]` for compactness.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    derive_more::Display,
    derive_more::IsVariant,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr,
)]
#[repr(u8)]
pub enum InvalidityReason {
    #[display("Transaction with the same hash was already included in announce chain")]
    AlreadyIncluded = 0,
    #[display("Transaction was not included within validity window and becomes outdated")]
    Outdated = 1,
    #[display("Transaction's destination actor not found")]
    UnknownDestination = 2,
    #[display("Transaction's value not zero")]
    NonZeroValue = 3,
}

/// [`TransactionStatusResolver`] determines the [`TransactionStatus`] for injected transactions for
/// specific announce and chain head.
pub struct TransactionStatusResolver<DB> {
    db: DB,
    chain_head: H256,
    recent_included_txs: HashSet<HashOf<InjectedTransaction>>,
    latest_states: ProgramStates,
}

impl<DB: OnChainStorageRO + AnnounceStorageRO + Storage> TransactionStatusResolver<DB> {
    pub fn new_for_announce(db: DB, chain_head: H256, announce: HashOf<Announce>) -> Result<Self> {
        // find last computed predecessor announce
        let mut last_computed_predecessor = announce;
        while !db.announce_meta(last_computed_predecessor).computed {
            last_computed_predecessor = db
                .announce(last_computed_predecessor)
                .ok_or_else(|| {
                    anyhow!("Cannot found announce {last_computed_predecessor} body in DB")
                })?
                .parent;
        }

        Ok(Self {
            recent_included_txs: Self::collect_recent_included_txs(&db, announce)?,
            latest_states: db
                .announce_program_states(last_computed_predecessor)
                .ok_or_else(|| {
                    anyhow!(
                        "Cannot find computed announce {last_computed_predecessor} programs states in db"
                    )
                })?,
            db,
            chain_head,
        })
    }

    /// Determine [`TransactionStatus`] for injected transaction.
    pub fn resolve(&self, tx: &SignedInjectedTransaction) -> Result<TransactionStatus> {
        let reference_block = tx.data().reference_block;

        if tx.data().value != 0 {
            return Ok(InvalidityReason::NonZeroValue.into());
        }

        if !self.is_reference_block_within_validity_window(reference_block)? {
            return Ok(InvalidityReason::Outdated.into());
        }

        if self.recent_included_txs.contains(&tx.data().to_hash()) {
            return Ok(InvalidityReason::AlreadyIncluded.into());
        }

        if !self.is_reference_block_on_current_branch(reference_block)? {
            return Ok(PendingStatus::NotOnCurrentBranch.into());
        }

        let Some(destination_state_hash) = self.latest_states.get(&tx.data().destination) else {
            return Ok(InvalidityReason::UnknownDestination.into());
        };

        let Some(state) = self.db.program_state(destination_state_hash.hash) else {
            anyhow::bail!(
                "program state not found for actor({}) by valid hash({})",
                tx.data().destination,
                destination_state_hash.hash
            )
        };

        if state.requires_init_message() {
            return Ok(PendingStatus::UninitializedDestination(tx.data().destination).into());
        }

        Ok(TransactionStatus::Valid)
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
                // Reach start announce is not correct case, because can be earlier announces with injected txs.
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

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        MaybeHashOf, SimpleBlockData, StateHashWithQueueSize,
        db::{AnnounceStorageRW, OnChainStorageRW},
        ecdsa::PrivateKey,
        injected::VALIDITY_WINDOW,
        mock::*,
    };
    use ethexe_db::Database;
    use ethexe_runtime_common::state::{ActiveProgram, Program, ProgramState};
    use gear_core::program::MemoryInfix;
    use gprimitives::ActorId;
    use std::collections::BTreeMap;

    fn mock_tx(reference_block: H256) -> SignedInjectedTransaction {
        let mut tx = InjectedTransaction::mock(());
        tx.reference_block = reference_block;
        tx.destination = ActorId::zero();

        SignedInjectedTransaction::create(PrivateKey::random(), tx).unwrap()
    }

    fn setup_announce(
        db: &Database,
        txs: Vec<SignedInjectedTransaction>,
        destination_initialized: bool,
        parent: HashOf<Announce>,
    ) -> HashOf<Announce> {
        let announce = Announce {
            parent,
            injected_transactions: txs,
            ..Announce::mock(())
        };
        let announce_hash = db.set_announce(announce);

        let mut state = ProgramState::zero();
        state.program = Program::Active(ActiveProgram {
            allocations_hash: MaybeHashOf::empty(),
            pages_hash: MaybeHashOf::empty(),
            memory_infix: MemoryInfix::new(0),
            initialized: destination_initialized,
        });
        let state_hash = db.write_program_state(state);

        let state = StateHashWithQueueSize {
            hash: state_hash,
            ..Default::default()
        };
        db.mutate_announce_meta(announce_hash, |meta| {
            meta.computed = true;
        });
        db.set_announce_program_states(announce_hash, BTreeMap::from([(ActorId::zero(), state)]));

        announce_hash
    }

    #[test]
    fn test_check_tx_validity() {
        let db = Database::memory();
        let chain = BlockChain::mock(100).setup(&db);

        let chain_head = chain.blocks[VALIDITY_WINDOW as usize].hash;
        let announce_hash = setup_announce(
            &db,
            vec![],
            true,
            chain.block_top_announce_hash(VALIDITY_WINDOW as usize - 1),
        );
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();

        for block in chain.blocks.iter().skip(1).take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(TransactionStatus::Valid, resolver.resolve(&tx).unwrap());
        }
    }

    #[test]
    fn test_check_tx_duplicate() {
        let db = Database::memory();
        let chain = BlockChain::mock(100).setup(&db);

        let chain_head = chain.blocks[9].hash;
        let tx = mock_tx(chain.blocks[5].hash);
        let announce_hash = setup_announce(
            &db,
            vec![tx.clone()],
            true,
            chain.block_top_announce_hash(8),
        );
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();

        assert_eq!(
            TransactionStatus::Invalid(InvalidityReason::AlreadyIncluded),
            resolver.resolve(&tx).unwrap()
        );
    }

    #[test]
    fn test_check_tx_outdated() {
        let db = Database::memory();
        let chain = BlockChain::mock(100).setup(&db);

        let chain_head = chain.blocks[(VALIDITY_WINDOW * 2) as usize].hash;
        let announce_hash = setup_announce(
            &db,
            vec![],
            true,
            chain.block_top_announce_hash((VALIDITY_WINDOW * 2) as usize - 1),
        );
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();

        for block in chain.blocks.iter().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TransactionStatus::Invalid(InvalidityReason::Outdated),
                resolver.resolve(&tx).unwrap()
            );
        }
    }

    #[test]
    fn test_check_tx_not_on_current_branch() {
        let db = Database::memory();
        let chain = BlockChain::mock(35).setup(&db);

        let mut blocks_branch2 = vec![];

        let mut parent = chain.blocks[10].hash;
        chain.blocks.iter().skip(9).for_each(|block| {
            let mut header = block.to_simple().header;
            header.parent_hash = parent;

            let hash = H256::random();
            db.set_block_header(hash, header);
            blocks_branch2.push(SimpleBlockData { hash, header });
            parent = hash;
        });

        let chain_head = chain.blocks[35].hash;
        let announce_hash = setup_announce(&db, vec![], true, chain.block_top_announce_hash(34));
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();

        for block in blocks_branch2.iter() {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TransactionStatus::Pending(PendingStatus::NotOnCurrentBranch),
                resolver.resolve(&tx).unwrap()
            );
        }

        for block in chain.blocks.iter().rev().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(TransactionStatus::Valid, resolver.resolve(&tx).unwrap());
        }
    }

    #[test]
    fn test_check_injected_tx_can_not_initialize_actor() {
        let db = Database::memory();
        let chain = BlockChain::mock(10).setup(&db);

        let chain_head = chain.blocks[9].hash;
        let tx = mock_tx(chain.blocks[5].hash);
        let announce_hash = setup_announce(&db, vec![], false, chain.block_top_announce_hash(8));
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();

        assert!(matches!(
            resolver.resolve(&tx).unwrap(),
            TransactionStatus::Pending(PendingStatus::UninitializedDestination(_)),
        ));
    }

    #[test]
    fn test_check_tx_with_non_zero_value() {
        let db = Database::memory();
        let chain = BlockChain::mock(10).setup(&db);

        let chain_head = chain.blocks[9].hash;
        let announce_hash = setup_announce(&db, vec![], false, chain.block_top_announce_hash(8));

        let tx = {
            let mut tx = InjectedTransaction::mock(());
            tx.value = 100;
            tx.reference_block = chain_head;
            tx
        };

        let signed_tx = SignedInjectedTransaction::create(PrivateKey::random(), tx).unwrap();
        let resolver =
            TransactionStatusResolver::new_for_announce(db, chain_head, announce_hash).unwrap();
        let status = resolver.resolve(&signed_tx).unwrap();
        assert!(status.unwrap_invalid().is_non_zero_value());
    }
}
