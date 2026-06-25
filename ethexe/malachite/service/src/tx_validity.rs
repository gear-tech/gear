// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! Per-injected-tx validity checks for the Malachite Block (MB) world.
//!
//! Used on both producer and validator sides so an MB whose
//! `Operation::Injected(..)` payload would fail compute is rejected
//! before it commits.

use anyhow::{Result, anyhow};
use ethexe_common::{
    HashOf, ProgramStates, SimpleBlockData,
    db::{GlobalsStorageRO, MbStorageRO, OnChainStorageRO},
    events::{BlockRequestEvent, RouterRequestEvent, router::ProgramCreatedEvent},
    gear::INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD,
    injected::{InjectedTransaction, SignedInjectedTransaction, VALIDITY_WINDOW},
    malachite::Operation,
};
use ethexe_db::Database;
use ethexe_runtime_common::state::Storage;
use gprimitives::{ActorId, H256};
use std::collections::HashSet;

/// Minimum executable balance a destination program must have to receive
/// an injected message: twice the panic-charge floor, at 100 value-per-gas.
pub const MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES: u128 =
    INJECTED_MESSAGE_PANIC_GAS_CHARGE_THRESHOLD as u128 * 100 * 2;

/// Outcome of [`TxValidityChecker::check_tx_validity`] for one injected
/// transaction. The producer drops non-`Valid` txs from the MB;
/// the validator rejects the whole MB on any non-`Valid`.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TxValidity {
    /// Transaction is valid and can be included in an MB.
    Valid,
    /// Already included in one of the previous `VALIDITY_WINDOW` MBs.
    Duplicate,
    /// `reference_block` is outside the validity window (or unknown).
    Outdated,
    /// `reference_block` is not on the local canonical Ethereum chain.
    NotOnCurrentBranch,
    /// Destination [`gprimitives::ActorId`] does not exist in
    /// `latest_states`.
    UnknownDestination,
    /// Destination program has not yet been initialised.
    UninitializedDestination,
    // TODO: #5083 support non-zero-value transactions.
    /// Non-zero `value` is not yet supported.
    NonZeroValue,
    /// Destination program's executable balance is below the floor.
    InsufficientBalanceForInjectedMessages,
}

/// Stateful checker scoped to (`chain_head`, `parent_mb`); caches the
/// dedup set and the latest computed program states at construction.
pub struct TxValidityChecker {
    /// DB for header lookups and ancestry walks.
    db: Database,
    /// Reference point for the validity-window and branch checks.
    chain_head: SimpleBlockData,
    /// Local-history fence; ancestry walks stop here.
    start_block_hash: H256,
    /// Hashes of txs included in recent MBs, for dedup.
    recent_included_txs: HashSet<HashOf<InjectedTransaction>>,
    /// Program states snapshot of the latest computed MB ancestor.
    latest_states: ProgramStates,
}

impl TxValidityChecker {
    /// Build a checker for an MB whose consensus-chain parent is
    /// `parent_mb_hash` (`H256::zero()` for genesis).
    pub fn new_for_mb(
        db: Database,
        chain_head: SimpleBlockData,
        parent_mb_hash: H256,
    ) -> Result<Self> {
        // Walk back to the most recent computed MB — that's the snapshot
        // whose `program_states` we can trust.
        let mut cursor = parent_mb_hash;
        while !cursor.is_zero() && !db.mb_meta(cursor).computed {
            let cb = db.mb_compact_block(cursor).ok_or_else(|| {
                anyhow!("MB {cursor} on the chain-walk has no compact-block row — DB invariant")
            })?;
            cursor = cb.parent;
        }

        // `cursor` is either a computed MB or the zero ancestor; both carry
        // a seeded `program_states` row.
        let latest_states = db.mb_program_states(cursor).ok_or_else(|| {
            anyhow!("MB {cursor} marked computed but has no program_states row — DB invariant")
        })?;

        let recent_included_txs = Self::collect_recent_included_txs(&db, parent_mb_hash)?;
        let start_block_hash = db.globals().start_block_hash;

        Ok(Self {
            db,
            chain_head,
            start_block_hash,
            recent_included_txs,
            latest_states,
        })
    }

    /// Determine [`TxValidity`] for one injected transaction.
    pub fn check_tx_validity(&self, tx: &SignedInjectedTransaction) -> Result<TxValidity> {
        let reference_block = tx.data().reference_block;

        if tx.data().value != 0 {
            return Ok(TxValidity::NonZeroValue);
        }

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
            anyhow::bail!(
                "program state not found for actor({}) by valid hash({})",
                tx.data().destination,
                destination_state_hash.hash
            );
        };

        if state.requires_init_message() {
            return Ok(TxValidity::UninitializedDestination);
        }

        if state.executable_balance < MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES {
            return Ok(TxValidity::InsufficientBalanceForInjectedMessages);
        }

        Ok(TxValidity::Valid)
    }

    fn is_reference_block_within_validity_window(&self, reference_block: H256) -> Result<bool> {
        let Some(reference_block_height) = self.db.block_header(reference_block).map(|h| h.height)
        else {
            return Ok(false);
        };

        let chain_head_height = self.chain_head.header.height;
        Ok(reference_block_height <= chain_head_height
            && reference_block_height.saturating_add(VALIDITY_WINDOW as u32) > chain_head_height)
    }

    fn is_reference_block_on_current_branch(&self, reference_block: H256) -> Result<bool> {
        let mut block_hash = self.chain_head.hash;
        for _ in 0..VALIDITY_WINDOW {
            if block_hash == reference_block {
                return Ok(true);
            }

            if block_hash == self.start_block_hash {
                // Hit the start-block fence — older history isn't tracked.
                return Ok(false);
            }

            block_hash = self
                .db
                .block_header(block_hash)
                .ok_or_else(|| anyhow!("Block header not found for hash: {block_hash}"))?
                .parent_hash;
        }

        Ok(false)
    }

    /// Collect hashes of every [`Operation::Injected`] in the last
    /// `VALIDITY_WINDOW` MBs, for the dedup set. A missing MB row on the
    /// walk is treated as the start of locally-tracked history.
    pub fn collect_recent_included_txs(
        db: &Database,
        parent_mb: H256,
    ) -> Result<HashSet<HashOf<InjectedTransaction>>> {
        let mut txs = HashSet::new();
        let mut mb_hash = parent_mb;
        for _ in 0..VALIDITY_WINDOW {
            if mb_hash.is_zero() {
                break;
            }
            let Some(cb) = db.mb_compact_block(mb_hash) else {
                // Walk fell off our locally-tracked history.
                break;
            };
            let Some(operations) = db.operations(cb.operations_hash) else {
                break;
            };
            for op in operations.into_iter() {
                if let Operation::Injected(signed) = op {
                    txs.insert(signed.data().to_hash());
                }
            }
            mb_hash = cb.parent;
        }
        Ok(txs)
    }
}

/// Programs "touched" by Ethereum events in the range
/// `(last_advanced_eb, advanced_eb]` along the canonical chain.
/// Returns an empty set when there is no new EB to walk.
///
/// Best-effort a-priori estimate that only serves the per-MB
/// touched-programs cap — do not rely on it for anything else.
pub fn eb_touched_programs(
    db: &Database,
    last_advanced_eb: H256,
    advanced_eb: H256,
) -> Result<HashSet<ActorId>> {
    if advanced_eb.is_zero() || advanced_eb == last_advanced_eb {
        return Ok(HashSet::new());
    }

    // Seed the known set with the programs of the latest computed MB.
    let latest_computed_mb = db.globals().latest_computed_mb_hash;
    let mut known: HashSet<ActorId> = db
        .mb_program_states(latest_computed_mb)
        .ok_or_else(|| {
            anyhow!(
                "no program_states for latest_computed_mb_hash {latest_computed_mb} — DB invariant"
            )
        })?
        .keys()
        .copied()
        .collect();

    // Collect blocks in (last_advanced_eb, advanced_eb], newest-first.
    // Both endpoints already passed quarantine, so the walk terminates
    // at `last_advanced_eb` within a few EBs; the start-block fence is
    // the safe fallback under a deeper-than-quarantine reorg.
    let mut chain = Vec::new();
    let start_block_hash = db.globals().start_block_hash;
    let mut current = advanced_eb;
    loop {
        if current == last_advanced_eb || current.is_zero() {
            break;
        }
        chain.push(current);
        if current == start_block_hash {
            // Local start-block fence — older history isn't tracked.
            break;
        }
        let header = db.block_header(current).ok_or_else(|| {
            anyhow!("eb_touched_programs: block header for {current} missing — DB invariant")
        })?;
        current = header.parent_hash;
    }

    // Process oldest-first so `ProgramCreatedEvent` populates `known`
    // before later `MirrorEvent`s referencing that actor.
    chain.reverse();

    let mut touched = HashSet::new();
    for block_hash in chain {
        let events = db.block_events(block_hash).ok_or_else(|| {
            anyhow!("eb_touched_programs: block_events for {block_hash} missing — DB invariant")
        })?;
        for event in events {
            match event.to_request() {
                Some(BlockRequestEvent::Router(RouterRequestEvent::ProgramCreated(
                    ProgramCreatedEvent { actor_id, .. },
                ))) => {
                    known.insert(actor_id);
                }
                Some(BlockRequestEvent::Mirror { actor_id, .. }) if known.contains(&actor_id) => {
                    touched.insert(actor_id);
                }
                _ => {}
            }
        }
    }

    Ok(touched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethexe_common::{
        MaybeHashOf, PrivateKey, SignedMessage, StateHashWithQueueSize,
        db::{CompactMb, MbStorageRW, OnChainStorageRW},
        gear_core::program::MemoryInfix,
        injected::InjectedTransaction,
        malachite::Operations,
        mock::{BlockChain, Mock, Tap},
    };
    use ethexe_runtime_common::state::{
        ActiveProgram, MessageQueueHashWithSize, Program, ProgramState,
    };
    use gprimitives::ActorId;

    // ------------------------------------------------------------------
    // Master-style helpers (announce → MB).
    // ------------------------------------------------------------------

    fn test_block_chain(len: u32) -> BlockChain {
        BlockChain::mock(len)
    }

    fn test_injected_transaction(
        reference_block: H256,
        destination: ActorId,
    ) -> InjectedTransaction {
        InjectedTransaction {
            destination,
            payload: vec![].try_into().unwrap(),
            value: 0,
            reference_block,
            salt: H256::random().0.to_vec().try_into().unwrap(),
        }
    }

    fn signed_tx(tx: InjectedTransaction) -> SignedInjectedTransaction {
        SignedMessage::create(PrivateKey::random(), tx).unwrap()
    }

    fn mock_tx(reference_block: H256) -> SignedInjectedTransaction {
        signed_tx(test_injected_transaction(reference_block, ActorId::zero()))
    }

    fn program_state(initialized: bool, executable_balance: u128) -> ProgramState {
        ProgramState {
            program: Program::Active(ActiveProgram {
                allocations_hash: MaybeHashOf::empty(),
                pages_hash: MaybeHashOf::empty(),
                memory_infix: MemoryInfix::new(0),
                initialized,
            }),
            canonical_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            injected_queue: MessageQueueHashWithSize {
                hash: MaybeHashOf::empty(),
                cached_queue_size: 0,
            },
            waitlist_hash: MaybeHashOf::empty(),
            stash_hash: MaybeHashOf::empty(),
            mailbox_hash: MaybeHashOf::empty(),
            balance: 0,
            executable_balance,
            outgoing_actions_counter: 0,
        }
    }

    /// Master's `setup_announce` adapted to MB world.
    ///
    /// Creates a fresh MB on top of `parent_mb`, gives it
    /// `injected_transactions` as its transactions blob (so the dedup
    /// walk picks them up), and seeds its `mb_program_states` with one
    /// destination program of `ActorId::zero()` whose `initialized`
    /// flag is set per argument. Marks the MB `computed` so the
    /// checker uses this snapshot as `latest_states`.
    fn setup_mb(
        db: &Database,
        injected_transactions: Vec<SignedInjectedTransaction>,
        destination_initialized: bool,
        parent_mb: H256,
    ) -> H256 {
        setup_mb_with_balance(
            db,
            injected_transactions,
            destination_initialized,
            MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES,
            parent_mb,
        )
    }

    fn setup_mb_with_balance(
        db: &Database,
        injected_transactions: Vec<SignedInjectedTransaction>,
        destination_initialized: bool,
        executable_balance: u128,
        parent_mb: H256,
    ) -> H256 {
        let ops = Operations::new(
            injected_transactions
                .into_iter()
                .map(Operation::Injected)
                .collect(),
        );
        let operations_hash = db.set_operations(ops);
        let mb_hash = H256::random();
        db.set_mb_compact_block(
            mb_hash,
            CompactMb {
                parent: parent_mb,
                height: u64::MAX / 2,
                operations_hash,
            },
        );

        let state_hash =
            db.write_program_state(program_state(destination_initialized, executable_balance));
        db.set_mb_program_states(
            mb_hash,
            ethexe_common::ProgramStates::from([(
                ActorId::zero(),
                StateHashWithQueueSize {
                    hash: state_hash,
                    canonical_queue_size: 0,
                    injected_queue_size: 0,
                },
            )]),
        );
        db.mutate_mb_meta(mb_hash, |meta| meta.computed = true);
        mb_hash
    }

    // ------------------------------------------------------------------
    // Ports of master's `tx_validation::tests::*`.
    // ------------------------------------------------------------------

    /// Port of master's `test_check_tx_validity`.
    #[test]
    fn test_check_tx_validity() {
        let db = Database::memory();
        let chain = test_block_chain(100).setup(&db);

        let chain_head = chain.blocks[VALIDITY_WINDOW as usize].to_simple();
        let parent_mb = setup_mb(
            &db,
            vec![],
            true,
            chain.mb_hash_at(VALIDITY_WINDOW as usize - 1),
        );
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        for block in chain.blocks.iter().skip(1).take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Valid,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }

    /// Port of master's `test_check_tx_duplicate`.
    #[test]
    fn test_check_tx_duplicate() {
        let db = Database::memory();
        let chain = test_block_chain(100).setup(&db);

        let chain_head = chain.blocks[9].to_simple();
        let tx = mock_tx(chain.blocks[5].hash);
        let parent_mb = setup_mb(&db, vec![tx.clone()], true, chain.mb_hash_at(8));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        assert_eq!(
            TxValidity::Duplicate,
            tx_checker.check_tx_validity(&tx).unwrap()
        );
    }

    /// Port of master's `test_check_tx_outdated`.
    #[test]
    fn test_check_tx_outdated() {
        let db = Database::memory();
        let chain = test_block_chain(100).setup(&db);

        let chain_head = chain.blocks[(VALIDITY_WINDOW * 2) as usize].to_simple();
        let parent_mb = setup_mb(
            &db,
            vec![],
            true,
            chain.mb_hash_at((VALIDITY_WINDOW * 2) as usize - 1),
        );
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        for block in chain.blocks.iter().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Outdated,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }

    /// Port of master's `test_check_tx_not_on_current_branch`.
    #[test]
    fn test_check_tx_not_on_current_branch() {
        let db = Database::memory();
        let chain = test_block_chain(35).setup(&db);

        // Fork at block 10 into a sibling branch of equal length.
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

        let chain_head = chain.blocks[35].to_simple();
        let parent_mb = setup_mb(&db, vec![], true, chain.mb_hash_at(34));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        for block in blocks_branch2.iter() {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::NotOnCurrentBranch,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
        for block in chain.blocks.iter().rev().take(VALIDITY_WINDOW as usize) {
            let tx = mock_tx(block.hash);
            assert_eq!(
                TxValidity::Valid,
                tx_checker.check_tx_validity(&tx).unwrap()
            );
        }
    }

    /// Port of master's `test_check_injected_tx_can_not_initialize_actor`.
    #[test]
    fn test_check_injected_tx_can_not_initialize_actor() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);

        let chain_head = chain.blocks[9].to_simple();
        let tx = mock_tx(chain.blocks[5].hash);
        let parent_mb = setup_mb(&db, vec![], false, chain.mb_hash_at(8));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        assert_eq!(
            TxValidity::UninitializedDestination,
            tx_checker.check_tx_validity(&tx).unwrap()
        );
    }

    /// Port of master's `test_check_injected_transaction_non_zero_value`.
    #[test]
    fn test_check_injected_transaction_non_zero_value() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);

        let chain_head = chain.blocks[9].to_simple();
        let tx = test_injected_transaction(chain.blocks[5].hash, ActorId::zero())
            .tap_mut(|tx| tx.value = 100);

        let parent_mb = setup_mb(&db, vec![], true, chain.mb_hash_at(8));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        assert_eq!(
            TxValidity::NonZeroValue,
            tx_checker.check_tx_validity(&signed_tx(tx)).unwrap()
        );
    }

    /// Port of master's `test_rejecting_unknown_reference_block`.
    #[test]
    fn test_rejecting_unknown_reference_block() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);

        let chain_head = chain.blocks[9].to_simple();
        let tx = test_injected_transaction(H256::zero(), ActorId::zero());

        let parent_mb = setup_mb(&db, vec![], true, chain.mb_hash_at(8));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        assert_eq!(
            TxValidity::Outdated,
            tx_checker.check_tx_validity(&signed_tx(tx)).unwrap()
        );
    }

    /// Port of master's `test_reach_start_block_in_branch_check`.
    ///
    /// `start_block_hash` is the local-history fence — older EBs aren't
    /// tracked. A tx anchored on an EB outside this fence cannot be
    /// proven to be on the current branch.
    #[test]
    fn test_reach_start_block_in_branch_check() {
        let db = Database::memory();
        let chain = test_block_chain(10)
            .tap_mut(|chain| {
                let blocks_head = chain.blocks.split_off(8);
                let _ = chain.blocks.split_off(1);
                chain.blocks.extend(blocks_head);
                chain.globals.start_block_hash = chain.blocks[1].hash;
            })
            .setup(&db);

        let chain_head = chain.blocks[3].to_simple();
        let tx = test_injected_transaction(chain.blocks[0].hash, ActorId::zero());

        let parent_mb = setup_mb(&db, vec![], true, chain.mb_hash_at(3));
        let tx_checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();

        assert_eq!(
            TxValidity::NotOnCurrentBranch,
            tx_checker.check_tx_validity(&signed_tx(tx)).unwrap()
        );
    }

    // ------------------------------------------------------------------
    // Extra MB-world cases not in master's announce-era set.
    // ------------------------------------------------------------------

    /// Programs whose `executable_balance` is below the floor must be
    /// rejected. Not in master's set; we have the same constant.
    #[test]
    fn insufficient_balance_is_rejected() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);

        let chain_head = chain.blocks[9].to_simple();
        let parent_mb = setup_mb_with_balance(
            &db,
            vec![],
            true,
            MIN_EXECUTABLE_BALANCE_FOR_INJECTED_MESSAGES - 1,
            chain.mb_hash_at(8),
        );
        let checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, parent_mb).unwrap();
        let tx = mock_tx(chain.blocks[5].hash);
        assert_eq!(
            checker.check_tx_validity(&tx).unwrap(),
            TxValidity::InsufficientBalanceForInjectedMessages,
        );
    }

    /// `parent_mb_hash == zero` → no `program_states` → every tx falls
    /// into [`TxValidity::UnknownDestination`].
    #[test]
    fn genesis_parent_has_empty_states_so_every_tx_unknown_destination() {
        let db = Database::memory();
        let chain = test_block_chain(2).setup(&db);
        let checker =
            TxValidityChecker::new_for_mb(db.clone(), chain.blocks[1].to_simple(), H256::zero())
                .unwrap();
        let tx = mock_tx(chain.blocks[1].hash);
        assert_eq!(
            checker.check_tx_validity(&tx).unwrap(),
            TxValidity::UnknownDestination,
        );
    }

    /// If the parent MB isn't computed, the checker walks back to the
    /// first computed ancestor and uses its `program_states`.
    #[test]
    fn walks_back_to_first_computed_ancestor_when_parent_not_computed() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);

        let mb_grand = setup_mb(&db, vec![], true, chain.mb_hash_at(8));
        let mb_parent = H256::random();
        let operations_hash = db.set_operations(Operations::new(vec![]));
        db.set_mb_compact_block(
            mb_parent,
            CompactMb {
                parent: mb_grand,
                height: u64::MAX / 2 + 1,
                operations_hash,
            },
        );
        // mb_parent's mb_meta.computed stays false → checker walks past it.

        let chain_head = chain.blocks[9].to_simple();
        let checker = TxValidityChecker::new_for_mb(db.clone(), chain_head, mb_parent).unwrap();
        let tx = mock_tx(chain.blocks[5].hash);
        assert_eq!(checker.check_tx_validity(&tx).unwrap(), TxValidity::Valid);
    }

    /// Pin evaluation order: NonZeroValue short-circuits ahead of all
    /// other checks. A tx that would fail multiple checks at once still
    /// surfaces the earliest reason.
    #[test]
    fn ordering_is_value_then_window_then_branch_then_dedup() {
        let db = Database::memory();
        let chain = test_block_chain(10).setup(&db);
        let parent_mb = setup_mb(&db, vec![], true, chain.mb_hash_at(8));
        let checker =
            TxValidityChecker::new_for_mb(db.clone(), chain.blocks[9].to_simple(), parent_mb)
                .unwrap();

        // value != 0 AND ref_block not in DB. NonZeroValue wins.
        let tx =
            test_injected_transaction(H256::random(), ActorId::zero()).tap_mut(|tx| tx.value = 1);
        assert_eq!(
            checker.check_tx_validity(&signed_tx(tx)).unwrap(),
            TxValidity::NonZeroValue,
        );
    }
}
