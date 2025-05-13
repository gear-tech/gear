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

use std::{
    thread_local,
    cell::{RefCell, Cell},
    rc::Rc,
    collections::{HashMap, BTreeMap},
};
use gear_common::ProgramId;
use accounts::{Balance, ACCOUNT_STORAGE};
use actors::{TestActor, ACTORS_STORAGE};
use bank::{BankBalance, BANK_ACCOUNTS};
use blocks::{BLOCK_INFO_STORAGE, BlockInfoStorageInner};

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
        let original = ACTORS_STORAGE
            .with_borrow_mut(|act_s| act_s
                .iter_mut()
                .map(|(id, actor)| {
                    // Exhausting cloning is used as intended for the overlay mode.
                    let actor_clone = unsafe { actor.clone_exhausting() };
                    (*id, actor_clone)
                })
                .collect()
            );
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
        aso
            .iter_mut()
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
