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
pub(crate) mod task_pool;
pub(crate) mod waitlist;

use accounts::{Balance, ACCOUNT_STORAGE};
use actors::{TestActor, ACTORS_STORAGE};
use bank::{BankBalance, BANK_ACCOUNTS};
use blocks::{BlockInfoStorageInner, BLOCK_INFO_STORAGE};
use gear_common::ProgramId;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashMap},
    rc::Rc,
    thread_local,
};

thread_local! {
    /// Overlay mode enabled flag.
    static OVERLAY_ENABLED: Cell<bool> = const { Cell::new(false) };
    static ACCOUNT_STORAGE_OVERLAY: RefCell<HashMap<ProgramId, Balance>> = RefCell::new(HashMap::new());
    static ACTORS_STORAGE_OVERLAY: RefCell<BTreeMap<ProgramId, TestActor>> = RefCell::new(Default::default());
    static BANK_ACCOUNTS_OVERLAY: RefCell<HashMap<ProgramId, BankBalance>> = RefCell::new(Default::default());
    static BLOCK_INFO_STORAGE_OVERLAY: BlockInfoStorageInner = Rc::new(RefCell::new(None));
}

/// Enables overlay mode.
///
/// If overlay is enabled, this function is no-op.
pub(crate) fn enable_overlay() {
    if overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|v| v.set(true));

    // Enable overlay for accounts storage.
    ACCOUNT_STORAGE_OVERLAY.with(|acc_so| {
        let original = ACCOUNT_STORAGE.with_borrow(|acc_s| acc_s.clone());
        acc_so.replace(original);
    });

    // Enable overlay for actors storage.
    ACTORS_STORAGE_OVERLAY.with(|aso| {
        let original = ACTORS_STORAGE.with_borrow_mut(|act_s| {
            act_s
                .iter_mut()
                .map(|(id, actor)| {
                    // Exhausting cloning is used as intended for the overlay mode.
                    let actor_clone = unsafe { actor.clone_exhausting() };
                    (*id, actor_clone)
                })
                .collect()
        });
        aso.replace(original);
    });

    // Enable overlay for bank storage.
    BANK_ACCOUNTS_OVERLAY.with(|bank_so| {
        let original = BANK_ACCOUNTS.with_borrow(|bank_s| bank_s.clone());
        bank_so.replace(original);
    });

    // Enable overlay for block info storage.
    BLOCK_INFO_STORAGE_OVERLAY.with(|biso| {
        let original = BLOCK_INFO_STORAGE.with(|bis| bis.borrow().clone());
        assert!(original.is_some(), "Block info storage must be initialized");

        biso.replace(original);
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

    // Disable overlay for accounts storage.
    ACCOUNT_STORAGE_OVERLAY.with_borrow_mut(|acc_so| {
        acc_so.clear();
    });

    // Disable overlay for actors storage.
    ACTORS_STORAGE_OVERLAY.with_borrow_mut(|aso| {
        aso.iter_mut()
            .filter(|(_, actor)| actor.is_mock_actor())
            .for_each(|(id, actor)| {
                // Exhausting cloning from overlay to return values back to the original storage.
                // Only mock values are handled, because by the `clone_exhausting` impl these are
                // the only ones that are taken from the original storage.
                let actor_clone = unsafe { actor.clone_exhausting() };
                ACTORS_STORAGE.with_borrow_mut(|act_s| {
                    let v = act_s.insert(*id, actor_clone);
                    debug_assert!(v.is_some());
                });
            });

        aso.clear();
    });

    // Disable overlay for bank storage.
    BANK_ACCOUNTS_OVERLAY.with_borrow_mut(|bank_so| {
        bank_so.clear();
    });

    // Disable overlay for block info storage.
    BLOCK_INFO_STORAGE_OVERLAY.with(|biso| {
        biso.borrow_mut().take();
    });
}

pub(crate) fn overlay_enabled() -> bool {
    OVERLAY_ENABLED.with(|v| v.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        state::{accounts::Accounts, actors::Actors, bank::Bank, blocks::BlocksManager},
        EXISTENTIAL_DEPOSIT, GAS_MULTIPLIER,
    };
    use gear_core::ids::ProgramId;

    #[test]
    fn overlay_works() {
        assert!(!overlay_enabled());

        // Fill the accounts storage.
        let predef_acc1 = ProgramId::from(42);
        let predef_acc2 = ProgramId::from(43);
        let predef_acc3 = ProgramId::from(44);
        Accounts::increase(predef_acc1, EXISTENTIAL_DEPOSIT * 1000);
        Accounts::increase(predef_acc2, EXISTENTIAL_DEPOSIT * 1000);
        Accounts::increase(predef_acc3, EXISTENTIAL_DEPOSIT * 1000);

        // Fill the actors storage.
        Actors::insert(predef_acc1, TestActor::Uninitialized(None, None));
        Actors::insert(predef_acc2, TestActor::Uninitialized(None, None));
        Actors::insert(predef_acc3, TestActor::Uninitialized(None, None));

        // Fill the bank storage.
        let bank = Bank::default();
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

        // Enable overlay mode.
        enable_overlay();
        assert!(overlay_enabled());

        // Adjust accounts storage:
        // - add a new account
        // - change existing ones
        let new_acc = ProgramId::from(45);
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
    }
}
