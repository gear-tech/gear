// This file is part of Gear.
//
// Copyright (C) 2021-2025 Gear Technologies Inc.
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

// States and state managers that are used to emulate gear runtime.

pub(crate) mod accounts;
pub(crate) mod actors;
pub(crate) mod bank;
pub(crate) mod blocks;
pub(crate) mod gas_tree;
pub(crate) mod mailbox;
pub(crate) mod nonce;
pub(crate) mod queue;
pub(crate) mod stash;
pub(crate) mod task_pool;
pub(crate) mod waitlist;

use actors::{TestActor, ACTORS_STORAGE};
use bank::{BankBalance, BANK_ACCOUNTS};
use blocks::{BlockInfoStorageInner, BLOCK_INFO_STORAGE, CURRENT_EPOCH_RANDOM};
use gear_core::{ids::ActorId, message::StoredDispatch};
use nonce::{ID_NONCE, MSG_NONCE};
use queue::DISPATCHES_QUEUE;
use stash::{DispatchStashType, DISPATCHES_STASH};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap, VecDeque},
    rc::Rc,
    thread_local,
};

thread_local! {
    /// Overlay mode enabled flag.
    static OVERLAY_ENABLED: Cell<bool> = const { Cell::new(false) };
    static ACTORS_STORAGE_OVERLAY: RefCell<BTreeMap<ActorId, TestActor>> = RefCell::new(Default::default());
    static BANK_ACCOUNTS_OVERLAY: RefCell<HashMap<ActorId, BankBalance>> = RefCell::new(Default::default());
    static BLOCK_INFO_STORAGE_OVERLAY: BlockInfoStorageInner = Rc::new(RefCell::new(None));
    static MSG_NONCE_OVERLAY: Cell<u64> = const { Cell::new(0) };
    static ID_NONCE_OVERLAY: Cell<u64> = const { Cell::new(0) };
    static DISPATCHES_QUEUE_OVERLAY: RefCell<VecDeque<StoredDispatch>> = const { RefCell::new(VecDeque::new()) };
    static CURRENT_EPOCH_RANDOM_OVERLAY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static DISPATCHES_STASH_OVERLAY: DispatchStashType = RefCell::new(HashMap::new());
}

/// Enables overlay mode.
///
/// If overlay is enabled, this function is no-op.
pub(crate) fn enable_overlay() {
    if overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|v| v.set(true));

    // Enable overlay for bank storage.
    BANK_ACCOUNTS_OVERLAY.with(|bank_so| {
        let original = BANK_ACCOUNTS.with_borrow(|bank_s| bank_s.clone());
        bank_so.replace(original);
    });

    // Enable overlay for block info storage.
    BLOCK_INFO_STORAGE_OVERLAY.with(|biso| {
        let original = BLOCK_INFO_STORAGE.with(|bis| *bis.borrow());
        assert!(original.is_some(), "Block info storage must be initialized");

        biso.replace(original);
    });

    // Enable overlay for message nonce storage.
    MSG_NONCE_OVERLAY.with(|msg_nonce| {
        let original = MSG_NONCE.with(|mn| mn.get());
        msg_nonce.set(original);
    });

    // Enable overlay for id nonce storage.
    ID_NONCE_OVERLAY.with(|id_nonce| {
        let original = ID_NONCE.with(|idn| idn.get());
        id_nonce.set(original);
    });

    // Enable overlay for dispatches queue storage.
    DISPATCHES_QUEUE_OVERLAY.with(|dq| {
        let original = DISPATCHES_QUEUE.with_borrow(|dqs| dqs.clone());
        dq.replace(original);
    });

    // Enable overlay for current epoch random storage.
    CURRENT_EPOCH_RANDOM_OVERLAY.with(|cero| {
        let original = CURRENT_EPOCH_RANDOM.with_borrow(|cer| cer.clone());
        cero.replace(original);
    });

    // Enable overlay for dispatches stash storage.
    DISPATCHES_STASH_OVERLAY.with(|dso| {
        let original = DISPATCHES_STASH.with_borrow(|ds| ds.clone());
        dso.replace(original);
    });
}

/// Disables overlay mode.
///
/// If overlay is disabled, this function is no-op.
pub(crate) fn disable_overlay() {
    if !overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|v| v.set(false));

    // Disable overlay for bank storage.
    BANK_ACCOUNTS_OVERLAY.with_borrow_mut(|bank_so| {
        bank_so.clear();
    });

    // Disable overlay for block info storage.
    BLOCK_INFO_STORAGE_OVERLAY.with(|biso| {
        biso.borrow_mut().take();
    });

    // Disable overlay for message nonce storage.
    MSG_NONCE_OVERLAY.with(|mno| {
        mno.set(0);
    });

    // Disable overlay for id nonce storage.
    ID_NONCE_OVERLAY.with(|ido| {
        ido.set(0);
    });

    // Disable overlay for dispatches queue storage.
    DISPATCHES_QUEUE_OVERLAY.with_borrow_mut(|dqo| {
        dqo.clear();
    });

    // Disable overlay for current epoch random storage.
    CURRENT_EPOCH_RANDOM_OVERLAY.with_borrow_mut(|cero| {
        cero.clear();
    });

    DISPATCHES_STASH_OVERLAY.with_borrow_mut(|dso| {
        dso.clear();
    });
}

pub(crate) fn overlay_enabled() -> bool {
    OVERLAY_ENABLED.with(|v| v.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        state::{
            accounts::Accounts, actors::Actors, bank::Bank, blocks::BlocksManager,
            nonce::NonceManager, queue::QueueManager, stash::DispatchStashManager,
        },
        EXISTENTIAL_DEPOSIT, GAS_MULTIPLIER,
    };
    use gear_common::storage::Interval;
    use gear_core::{
        ids::{ActorId, MessageId},
        message::{DispatchKind, StoredDelayedDispatch},
    };

    #[test]
    fn overlay_works() {
        assert!(!overlay_enabled());

        // Fill the accounts storage.
        let predef_acc1 = ActorId::from(42);
        let predef_acc2 = ActorId::from(43);
        let predef_acc3 = ActorId::from(44);
        Accounts::increase(predef_acc1, EXISTENTIAL_DEPOSIT * 1000);
        Accounts::increase(predef_acc2, EXISTENTIAL_DEPOSIT * 1000);
        Accounts::increase(predef_acc3, EXISTENTIAL_DEPOSIT * 1000);

        // Fill the actors storage.
        Actors::insert(predef_acc1, TestActor::Uninitialized(None, None));
        Actors::insert(predef_acc2, TestActor::Uninitialized(None, None));
        Actors::insert(predef_acc3, TestActor::Uninitialized(None, None));

        // Fill the bank storage.
        let bank = Bank;
        bank.deposit_gas(predef_acc1, 1_000, true);
        bank.deposit_gas(predef_acc2, 1_000 * 2, true);
        bank.deposit_gas(predef_acc3, 1_000 * 3, true);

        bank.deposit_value(predef_acc1, EXISTENTIAL_DEPOSIT, true);
        bank.deposit_value(predef_acc2, EXISTENTIAL_DEPOSIT * 2, true);
        bank.deposit_value(predef_acc3, EXISTENTIAL_DEPOSIT * 3, true);

        let acc1_balance_before_overlaid = Accounts::balance(predef_acc1);
        let acc2_balance_before_overlaid = Accounts::balance(predef_acc2);
        let acc3_balance_before_overlaid = Accounts::balance(predef_acc3);

        // Fill the block info storage.
        let bm = BlocksManager::new();
        bm.next_block();
        assert_eq!(bm.get().height, 1);

        // Fill the message nonce storage.
        let nm = NonceManager;
        nm.fetch_inc_message_nonce();
        let message_nonce_before_overlay = nm.fetch_inc_message_nonce();

        nm.inc_id_nonce();
        nm.inc_id_nonce();
        let id_nonce_before_overlay = nm.id_nonce();

        // Fill the dispatches queue storage.
        let qm = QueueManager;
        let dispatch = StoredDispatch::new(DispatchKind::Init, Default::default(), None);
        qm.push_back(dispatch.clone());
        qm.push_back(dispatch.clone());
        qm.push_back(dispatch.clone());

        let epoch_random_before_overlay = blocks::current_epoch_random();

        // Fill the dispatch stash storage.
        let dsm = DispatchStashManager;
        let mid1 = MessageId::from(52);
        let mid2 = MessageId::from(53);
        let stash_value = (
            StoredDelayedDispatch::new(DispatchKind::Init, Default::default()),
            Interval {
                start: 0,
                finish: 10,
            },
        );
        dsm.insert(mid1, stash_value.clone());
        dsm.insert(mid2, stash_value.clone());

        // Enable overlay mode.
        enable_overlay();
        assert!(overlay_enabled());

        // Adjust accounts storage:
        // - add a new account
        // - change existing ones
        let new_acc = ActorId::from(45);
        Accounts::increase(new_acc, EXISTENTIAL_DEPOSIT * 1000);
        Accounts::decrease(predef_acc1, EXISTENTIAL_DEPOSIT, true);
        Accounts::decrease(predef_acc2, EXISTENTIAL_DEPOSIT, true);

        let new_acc1_balance_overlaid = Accounts::balance(new_acc);
        let predef_acc1_balance_overlaid = Accounts::balance(predef_acc1);
        let predef_acc2_balance_overlaid = Accounts::balance(predef_acc2);

        assert_eq!(
            predef_acc1_balance_overlaid,
            acc1_balance_before_overlaid - EXISTENTIAL_DEPOSIT
        );
        assert_eq!(
            predef_acc2_balance_overlaid,
            acc2_balance_before_overlaid - EXISTENTIAL_DEPOSIT
        );
        assert_eq!(new_acc1_balance_overlaid, EXISTENTIAL_DEPOSIT * 1000);

        // Adjust actors storage the same way.
        let acc2_actor_ty = TestActor::CodeNotExists;
        let acc3_actor_ty = TestActor::FailedInit;
        Actors::insert(new_acc, TestActor::Uninitialized(None, None));
        Actors::modify(predef_acc1, |actor| {
            *actor.expect("checked") = acc2_actor_ty;
        });
        Actors::modify(predef_acc2, |actor| {
            *actor.expect("checked") = acc3_actor_ty;
        });

        // Adjust bank storage the same way.
        bank.deposit_gas(new_acc, 1_000, true);
        bank.deposit_value(new_acc, EXISTENTIAL_DEPOSIT, true);

        bank.spend_gas(predef_acc1, 200, GAS_MULTIPLIER);
        bank.transfer_value(predef_acc1, new_acc, EXISTENTIAL_DEPOSIT);

        // Assert balances
        let new_acc1_balance_overlaid_after_bank = Accounts::balance(new_acc);

        assert_eq!(
            new_acc1_balance_overlaid_after_bank,
            new_acc1_balance_overlaid - EXISTENTIAL_DEPOSIT - GAS_MULTIPLIER.gas_to_value(1_000)
                + EXISTENTIAL_DEPOSIT
        );

        // Adjust blocks storage
        bm.move_blocks_by(10);
        assert_eq!(bm.get().height, 11);

        // Adjust nonces
        nm.fetch_inc_message_nonce();
        let latest_message_nonce = nm.fetch_inc_message_nonce();

        nm.inc_id_nonce();
        nm.inc_id_nonce();
        let latest_id_nonce = nm.id_nonce();

        assert_eq!(latest_message_nonce, message_nonce_before_overlay + 2);
        assert_eq!(latest_id_nonce, id_nonce_before_overlay + 2);

        // Adjust dispatches queue storage
        qm.pop_front();
        qm.pop_front();
        qm.push_front(StoredDispatch::new(
            DispatchKind::Handle,
            Default::default(),
            None,
        ));

        // Adjust current epoch random storage
        blocks::update_epoch_random(42424242);
        let overlaid_random = blocks::current_epoch_random();
        assert_ne!(overlaid_random, epoch_random_before_overlay);

        // Adjust dispatches stash storage
        let mid3 = MessageId::from(54);
        dsm.insert(mid3, stash_value.clone());
        assert!(dsm.remove(&mid1).is_some());

        // Disable overlay mode.
        disable_overlay();

        // New acc doesn't exist.
        assert_eq!(Accounts::balance(new_acc), 0);
        assert!(!Actors::contains_key(new_acc));

        // Balances hasn't changed.
        assert_eq!(Accounts::balance(predef_acc1), acc1_balance_before_overlaid);
        assert_eq!(Accounts::balance(predef_acc2), acc2_balance_before_overlaid);
        assert_eq!(Accounts::balance(predef_acc3), acc3_balance_before_overlaid);

        // Actors haven't changed.
        let check_actor = |id| {
            Actors::access(id, |a| {
                assert!(matches!(a, Some(TestActor::Uninitialized(None, None))));
            });
        };
        check_actor(predef_acc1);
        check_actor(predef_acc2);
        check_actor(predef_acc3);

        // Block info storage hasn't changed.
        assert_eq!(bm.get().height, 1);

        // Nonces haven't changed.
        assert_eq!(
            nm.fetch_inc_message_nonce(),
            message_nonce_before_overlay + 1
        );
        assert_eq!(nm.id_nonce(), id_nonce_before_overlay);

        // Dispatches queue storage hasn't changed.
        assert_eq!(qm.len(), 3);
        assert_eq!(qm.pop_front().expect("len checked"), dispatch);

        // Current epoch random storage hasn't changed.
        assert_eq!(blocks::current_epoch_random(), epoch_random_before_overlay);

        // Dispatches stash storage hasn't changed.
        assert!(dsm.contains_key(&mid1));
        assert!(!dsm.contains_key(&mid3));
    }
}
