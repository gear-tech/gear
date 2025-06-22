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

use std::cell::{Cell, Ref, RefCell, RefMut};

std::thread_local! {
    /// Overlay mode enabled flag.
    static OVERLAY_ENABLED: Cell<bool> = const { Cell::new(false) };
}

// todo [sab] clear
pub struct WithOverlay<T> {
    original: RefCell<T>,
    overlay: RefCell<T>,
    set_overlay: Cell<bool>,
}

impl<T: Default> Default for WithOverlay<T> {
    fn default() -> Self {
        Self {
            original: Default::default(),
            overlay: Default::default(),
            set_overlay: Default::default(),
        }
    }
}

impl<T: Clone> WithOverlay<T> {
    pub fn new(original: T) -> Self
    where
        T: Default,
    {
        Self {
            original: RefCell::new(original.clone()),
            overlay: RefCell::new(Default::default()),
            set_overlay: Cell::new(false),
        }
    }

    pub fn data(&self) -> Ref<'_, T> {
        self.prepare_data();

        if overlay_enabled() {
            self.overlay.borrow()
        } else {
            self.original.borrow()
        }
    }

    pub fn data_mut(&self) -> RefMut<'_, T> {
        self.prepare_data();

        if overlay_enabled() {
            self.overlay.borrow_mut()
        } else {
            self.original.borrow_mut()
        }
    }

    fn prepare_data(&self) {
        if overlay_enabled() {
            let overlay_is_set = self.set_overlay.get();
            if !overlay_is_set {
                let original = self.original.borrow().clone();
                self.overlay.replace(original);

                self.set_overlay.set(true);
            }
        } else {
            let overlay_is_set = self.set_overlay.get();
            if overlay_is_set {
                self.set_overlay.set(false);
            }
        }
    }
}

/// Enables overlay mode for the storage.
///
/// If overlay mode is already enabled, it's no-op.
pub fn enable_overlay() {
    if overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|oe| oe.set(true));
}

/// Disables overlay mode for the storage.
///
/// If overlay mode is already disabled, it's no-op.
pub fn disable_overlay() {
    if !overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|oe| oe.set(false));
}

/// Checks if overlay mode is enabled.
pub fn overlay_enabled() -> bool {
    OVERLAY_ENABLED.with(|oe| oe.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        auxiliary::{
            gas_provider::{GasNodesWrap, Node, NodeId, TotalIssuanceWrap},
            mailbox::{MailboxStorageWrap, MailboxedMessage},
            task_pool::TaskPoolStorageWrap,
            waitlist::{WaitlistStorageWrap, WaitlistedMessage},
        },
        storage::{DoubleMapStorage, Interval, MapStorage, ValueStorage},
        GasMultiplier, Origin,
    };
    use gear_core::{message::DispatchKind, tasks::VaraScheduledTask};
    use sp_core::H256;

    #[test]
    fn overlay_works() {
        assert!(!overlay_enabled());

        // Fill gas tree storage with some data.
        let node1_id = NodeId::Node(H256::random().cast());
        let node2_id = NodeId::Node(H256::random().cast());
        let node3_id = NodeId::Node(H256::random().cast());

        let ext_id1 = H256::random().cast();
        let ext_id2 = H256::random().cast();

        let node1_value = 1_000_000;
        let node2_value = 1_000_000;
        let node3_value = 100_000;

        let multiplier = GasMultiplier::ValuePerGas(100);

        GasNodesWrap::insert(node1_id, Node::new(ext_id1, multiplier, 1_000_000, false));
        GasNodesWrap::insert(node2_id, Node::new(ext_id2, multiplier, 1_000_000, false));
        GasNodesWrap::insert(
            node3_id,
            Node::SpecifiedLocal {
                parent: node2_id,
                root: node2_id,
                value: node3_value,
                lock: Default::default(),
                system_reserve: Default::default(),
                refs: Default::default(),
                consumed: Default::default(),
            },
        );

        let total_issuance = node1_value + node2_value + node3_value;
        TotalIssuanceWrap::put(total_issuance);

        // Fill the mailbox storage with some data.
        let pid1 = H256::random().cast();
        let pid2 = H256::random().cast();
        let mid1 = H256::random().cast();
        let mid2 = H256::random().cast();
        MailboxStorageWrap::insert(
            pid1,
            mid1,
            (
                Default::default(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );
        MailboxStorageWrap::insert(
            pid2,
            mid2,
            (
                Default::default(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );

        // Fill the task pool storage with some data.
        let task1_bn = 5;
        let task2_bn = 6;
        let task1 = VaraScheduledTask::WakeMessage(H256::random().cast(), H256::random().cast());
        let task2 = VaraScheduledTask::WakeMessage(H256::random().cast(), H256::random().cast());

        TaskPoolStorageWrap::insert(task1_bn, task1.clone(), ());
        TaskPoolStorageWrap::insert(task2_bn, task2.clone(), ());

        // Fill the waitlist storage with some data.
        let waitlist_key1_1 = H256::random().cast();
        let waitlist_key1_2 = H256::random().cast();
        let waitlist_key2_1 = H256::random().cast();
        let waitlist_key2_2 = H256::random().cast();
        let waitlisted_msg = WaitlistedMessage::new(DispatchKind::Init, Default::default(), None);

        WaitlistStorageWrap::insert(
            waitlist_key1_1,
            waitlist_key1_2,
            (
                waitlisted_msg.clone(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );
        WaitlistStorageWrap::insert(
            waitlist_key2_1,
            waitlist_key2_2,
            (
                waitlisted_msg.clone(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );

        // Enable overlay mode.
        enable_overlay();
        assert!(overlay_enabled());

        // Adjust gas tree storage by adding a new one, modifying existing one, and removing one.
        let node4_id = NodeId::Node(H256::random().cast());
        let node4_value = 200_000;
        GasNodesWrap::insert(
            node4_id,
            Node::SpecifiedLocal {
                parent: node2_id,
                root: node2_id,
                value: node4_value,
                lock: Default::default(),
                system_reserve: Default::default(),
                refs: Default::default(),
                consumed: Default::default(),
            },
        );
        assert!(GasNodesWrap::take(node2_id).is_some());
        GasNodesWrap::insert(node1_id, Node::new(ext_id1, multiplier, 5_000_000, false));

        TotalIssuanceWrap::put(42);

        // Adjust mailbox storage the same way.
        let pid3 = H256::random().cast();
        let mid3 = H256::random().cast();

        let mid2_new = H256::random().cast();
        MailboxStorageWrap::insert(
            pid3,
            mid3,
            (
                Default::default(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );
        assert!(MailboxStorageWrap::take(pid1, mid1).is_some());
        MailboxStorageWrap::mutate(pid2, mid2, |maybe_m| {
            if let Some((m, _)) = maybe_m {
                let new_m = MailboxedMessage::new(
                    mid2_new,
                    m.source(),
                    m.destination(),
                    m.payload_bytes().try_into().unwrap(),
                    m.value(),
                );
                *m = new_m;
            }
        });

        // Adjust task pool storage the same way.
        let task3_bn = 7;
        let task3 = VaraScheduledTask::WakeMessage(H256::random().cast(), H256::random().cast());

        TaskPoolStorageWrap::insert(task3_bn, task3.clone(), ());
        assert!(TaskPoolStorageWrap::take(task1_bn, task1.clone()).is_some());

        // Adjust waitlist storage the same way.
        let waitlist_key3_1 = H256::random().cast();
        let waitlist_key3_2 = H256::random().cast();

        WaitlistStorageWrap::insert(
            waitlist_key3_1,
            waitlist_key3_2,
            (
                waitlisted_msg.clone(),
                Interval {
                    start: 0,
                    finish: 10,
                },
            ),
        );
        assert!(WaitlistStorageWrap::take(waitlist_key1_1, waitlist_key1_2).is_some());
        WaitlistStorageWrap::mutate(waitlist_key2_1, waitlist_key2_2, |maybe_m| {
            if let Some((m, _)) = maybe_m {
                let new_m = WaitlistedMessage::new(DispatchKind::Handle, Default::default(), None);
                *m = new_m;
            }
        });

        // Disable overlay mode.
        disable_overlay();

        // Check gas tree storage not changed
        assert!(GasNodesWrap::get(&node4_id).is_none());
        assert!(GasNodesWrap::get(&node2_id).is_some());
        let node1 = GasNodesWrap::get(&node1_id).expect("internal error: node not found");
        assert_eq!(node1.value().expect("external node has value"), node1_value);
        let total_issuance_actual =
            TotalIssuanceWrap::get().expect("internal error: total issuance not found");
        assert_eq!(total_issuance_actual, total_issuance);

        // Check mailbox storage not changed
        assert!(MailboxStorageWrap::get(&pid3, &mid3).is_none());
        assert!(MailboxStorageWrap::get(&pid1, &mid1).is_some());
        let (mailbox_msg2, _) = MailboxStorageWrap::get(&pid2, &mid2)
            .expect("internal error: mailbox message not found");
        assert_eq!(mailbox_msg2.id(), Default::default());

        // Check task pool storage not changed
        assert!(TaskPoolStorageWrap::get(&task1_bn, &task1).is_some());
        assert!(TaskPoolStorageWrap::get(&task3_bn, &task3).is_none());

        // Check waitlist storage not changed
        assert!(WaitlistStorageWrap::get(&waitlist_key1_1, &waitlist_key1_2).is_some());
        assert!(WaitlistStorageWrap::get(&waitlist_key3_1, &waitlist_key3_2).is_none());
        let (waitlisted_msg2, _) = WaitlistStorageWrap::get(&waitlist_key2_1, &waitlist_key2_2)
            .expect("internal error: waitlisted message not found");
        assert_eq!(waitlisted_msg2.kind(), DispatchKind::Init);
    }
}
