// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Storage overlay implementation for auxiliary storage managers.
//!
//! The real runtime has an ability to run gear runtime exported functions
//! inside the overlay, which won't modify the real storage. Same functionality
//! is provided within the module

use crate::auxiliary::{
    gas_provider::{Balance, Node, NodeId, GAS_NODES, TOTAL_ISSUANCE},
    mailbox::{MailboxStorage, MAILBOX_STORAGE},
    task_pool::{TaskPoolStorage, TASKPOOL_STORAGE},
    waitlist::{WaitlistStorage, WAITLIST_STORAGE},
    DoubleBTreeMap,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

std::thread_local! {
    /// Overlay mode enabled flag.
    static OVERLAY_ENABLED: Cell<bool> = const { Cell::new(false) };
    /// Overlay copy of the `TOTAL_ISSUANCE` of gas tree storage.
    pub(crate) static TOTAL_ISSUANCE_OVERLAY: RefCell<Option<Balance>> = const { RefCell::new(None) };
    /// Overlay copy of the `GAS_NODES` of gas tree storage.
    pub(crate) static GAS_NODES_OVERLAY: RefCell<BTreeMap<NodeId, Node>> = const { RefCell::new(BTreeMap::new()) };
    /// Overlay copy of the mailbox storage.
    pub(crate) static MAILBOX_OVERLAY: MailboxStorage = const { RefCell::new(DoubleBTreeMap::new()) };
    /// Overlay copy of the task pool storage.
    pub(crate) static TASKPOOL_OVERLAY: TaskPoolStorage = const { RefCell::new(DoubleBTreeMap::new()) };
    /// Overlay copy of the waitlist storage.
    pub(crate) static WAITLIST_OVERLAY: WaitlistStorage = const { RefCell::new(DoubleBTreeMap::new()) };
}

/// Enables overlay mode for the storage.
///
/// If overlay mode is already enabled, it's no-op.
pub fn enable_overlay() {
    if overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|oe| oe.set(true));

    // Enable overlay for the gas tree.
    TOTAL_ISSUANCE_OVERLAY.with(|tio| {
        let ti_value = TOTAL_ISSUANCE.with_borrow(|i| *i);
        tio.replace(ti_value);
    });
    GAS_NODES_OVERLAY.with(|gn_overlay| {
        let gn_map = GAS_NODES.with_borrow(|gn| gn.clone());
        gn_overlay.replace(gn_map);
    });

    // Enable overlay for the mailbox.
    MAILBOX_OVERLAY.with(|mo| {
        let original = MAILBOX_STORAGE.with_borrow(|m| m.clone());
        mo.replace(original);
    });

    // Enable overlay for the task pool.
    TASKPOOL_OVERLAY.with(|tpo| {
        let original = TASKPOOL_STORAGE.with_borrow(|t| t.clone());
        tpo.replace(original);
    });

    // Enable overlay for the waitlist.
    WAITLIST_OVERLAY.with(|wo| {
        let original = WAITLIST_STORAGE.with_borrow(|w| w.clone());
        wo.replace(original);
    });
}

/// Disables overlay mode for the storage.
///
/// If overlay mode is already disabled, it's no-op.
pub fn disable_overlay() {
    if !overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|oe| oe.set(false));

    // Disable overlay for the gas tree.
    TOTAL_ISSUANCE_OVERLAY.with_borrow_mut(|tio| {
        *tio = None;
    });
    GAS_NODES_OVERLAY.with_borrow_mut(|gno| {
        gno.clear();
    });

    // Disable overlay for the mailbox.
    MAILBOX_OVERLAY.with_borrow_mut(|mo| {
        mo.clear();
    });

    // Disable overlay for the task pool.
    TASKPOOL_OVERLAY.with_borrow_mut(|tpo| {
        tpo.clear();
    });

    // Disable overlay for the waitlist.
    WAITLIST_OVERLAY.with_borrow_mut(|wo| {
        wo.clear();
    });
}

/// Checks if overlay mode is enabled.
pub fn overlay_enabled() -> bool {
    OVERLAY_ENABLED.with(|oe| oe.get())
}
