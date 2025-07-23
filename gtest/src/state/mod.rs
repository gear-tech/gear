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
pub(crate) mod bank;
pub(crate) mod blocks;
pub(crate) mod gas_tree;
pub(crate) mod mailbox;
pub(crate) mod nonce;
pub(crate) mod programs;
pub(crate) mod queue;
pub(crate) mod stash;
pub(crate) mod task_pool;
pub(crate) mod waitlist;

use std::{
    cell::{Cell, Ref, RefCell, RefMut},
    thread_local,
};

#[derive(Debug)]
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

thread_local! {
    /// Overlay mode enabled flag.
    static OVERLAY_ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Enables overlay mode.
///
/// If overlay is enabled, this function is no-op.
pub(crate) fn enable_overlay() {
    if overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|v| v.set(true));
}

/// Disables overlay mode.
///
/// If overlay is disabled, this function is no-op.
pub(crate) fn disable_overlay() {
    if !overlay_enabled() {
        return;
    }

    OVERLAY_ENABLED.with(|v| v.set(false));
}

pub(crate) fn overlay_enabled() -> bool {
    OVERLAY_ENABLED.with(|v| v.get())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BlockNumber, EXISTENTIAL_DEPOSIT, GAS_MULTIPLIER,
        state::{
            accounts::Accounts,
            bank::Bank,
            blocks::BlocksManager,
            gas_tree::{
                GTestGasNodesProvider, GTestGasNodesStorage, GTestTotalIssuanceProvider,
                GTestTotalIssuanceStorage,
            },
            mailbox::manager::{MailboxStorageWrap, MailboxedMessage},
            nonce::NonceManager,
            programs::{PLACEHOLDER_MESSAGE_ID, ProgramsStorageManager},
            queue::QueueManager,
            stash::DispatchStashManager,
            task_pool::TaskPoolStorageWrap,
            waitlist::{WaitlistStorageWrap, WaitlistedMessage},
        },
    };
    use gear_common::{
        ActiveProgram, GasMultiplier, Origin, Program,
        gas_provider::auxiliary::{GasNodesWrap, Node, NodeId, TotalIssuanceWrap},
        storage::{DoubleMapStorage, Interval, MapStorage, ValueStorage},
    };
    use gear_core::{
        ids::{ActorId, MessageId},
        message::{
            DispatchKind, StoredDelayedDispatch, StoredDispatch, StoredMessage, UserStoredMessage,
        },
        program::ProgramState,
        tasks::VaraScheduledTask,
    };
    use sp_core::H256;

    fn default_stored_message() -> StoredMessage {
        StoredMessage::new(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    fn default_user_stored_message() -> UserStoredMessage {
        UserStoredMessage::new(
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    fn create_active_program() -> ActiveProgram<BlockNumber> {
        ActiveProgram {
            allocations_tree_len: 0,
            memory_infix: Default::default(),
            gas_reservation_map: Default::default(),
            code_id: H256::random().cast(),
            state: ProgramState::Uninitialized {
                message_id: PLACEHOLDER_MESSAGE_ID,
            },
            expiration_block: 100,
        }
    }

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

        let prog1 = create_active_program();
        let prog2 = create_active_program();
        let prog3 = create_active_program();
        let code_id1 = prog1.code_id;
        let code_id2 = prog2.code_id;
        let code_id3 = prog3.code_id;

        // Fill the actors storage.
        ProgramsStorageManager::insert_program(predef_acc1, Program::Active(prog1));
        ProgramsStorageManager::insert_program(predef_acc2, Program::Active(prog2));
        ProgramsStorageManager::insert_program(predef_acc3, Program::Active(prog3));

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
        let bm = BlocksManager;
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
        let dispatch = StoredDispatch::new(DispatchKind::Init, default_stored_message(), None);
        qm.push_back(dispatch.clone());
        qm.push_back(dispatch.clone());
        qm.push_back(dispatch.clone());

        let epoch_random_before_overlay = blocks::current_epoch_random();

        // Fill the dispatch stash storage.
        let dsm = DispatchStashManager;
        let mid1 = MessageId::from(52);
        let mid2 = MessageId::from(53);
        let stash_value = (
            StoredDelayedDispatch::new(DispatchKind::Init, default_stored_message()),
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
        let acc2_actor_ty = Program::Exited(H256::random().cast());
        let acc3_actor_ty = Program::Terminated(H256::random().cast());
        ProgramsStorageManager::insert_program(new_acc, Program::Active(create_active_program()));
        ProgramsStorageManager::modify_program(predef_acc1, |actor| {
            *actor.expect("checked") = acc2_actor_ty;
        });
        ProgramsStorageManager::modify_program(predef_acc2, |actor| {
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
            default_stored_message(),
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
        assert!(!ProgramsStorageManager::has_program(new_acc));

        // Balances hasn't changed.
        assert_eq!(Accounts::balance(predef_acc1), acc1_balance_before_overlaid);
        assert_eq!(Accounts::balance(predef_acc2), acc2_balance_before_overlaid);
        assert_eq!(Accounts::balance(predef_acc3), acc3_balance_before_overlaid);

        // Actors haven't changed.
        let check_actor = |idx, id, code_id_expected| {
            ProgramsStorageManager::access_program(id, |a| {
                let Some(Program::Active(active_program)) = a else {
                    panic!("Expected active program for actor {id}");
                };

                assert_eq!(
                    active_program.code_id, code_id_expected,
                    "failed check for test {idx}"
                );
                assert_eq!(
                    active_program.state,
                    ProgramState::Uninitialized {
                        message_id: PLACEHOLDER_MESSAGE_ID
                    },
                    "failed check for test {idx}"
                );
            });
        };
        for (idx, (acc, code_id)) in [
            (predef_acc1, code_id1),
            (predef_acc2, code_id2),
            (predef_acc3, code_id3),
        ]
        .into_iter()
        .enumerate()
        {
            check_actor(idx + 1, acc, code_id);
        }

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

    #[test]
    fn common_storages_overlay_works() {
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

        GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::insert(
            node1_id,
            Node::new(ext_id1, multiplier, 1_000_000, false),
        );
        GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::insert(
            node2_id,
            Node::new(ext_id2, multiplier, 1_000_000, false),
        );
        GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::insert(
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
        TotalIssuanceWrap::<GTestTotalIssuanceStorage, GTestTotalIssuanceProvider>::put(
            total_issuance,
        );

        // Fill the mailbox storage with some data.
        let pid1 = H256::random().cast();
        let pid2 = H256::random().cast();
        let mid1 = H256::random().cast();
        let mid2 = H256::random().cast();
        MailboxStorageWrap::insert(
            pid1,
            mid1,
            (
                default_user_stored_message(),
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
                default_user_stored_message(),
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
        let waitlisted_msg =
            WaitlistedMessage::new(DispatchKind::Init, default_stored_message(), None);

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

        // Adjust gas tree storage by adding a new one, modifying existing one, and
        // removing one.
        let node4_id = NodeId::Node(H256::random().cast());
        let node4_value = 200_000;
        GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::insert(
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
        assert!(
            GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::take(node2_id).is_some()
        );
        GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::insert(
            node1_id,
            Node::new(ext_id1, multiplier, 5_000_000, false),
        );

        TotalIssuanceWrap::<GTestTotalIssuanceStorage, GTestTotalIssuanceProvider>::put(42);

        // Adjust mailbox storage the same way.
        let pid3 = H256::random().cast();
        let mid3 = H256::random().cast();

        let mid2_new = H256::random().cast();
        MailboxStorageWrap::insert(
            pid3,
            mid3,
            (
                default_user_stored_message(),
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
                let new_m =
                    WaitlistedMessage::new(DispatchKind::Handle, default_stored_message(), None);
                *m = new_m;
            }
        });

        // Disable overlay mode.
        disable_overlay();

        // Check gas tree storage not changed
        assert!(
            GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::get(&node4_id).is_none()
        );
        assert!(
            GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::get(&node2_id).is_some()
        );
        let node1 = GasNodesWrap::<GTestGasNodesStorage, GTestGasNodesProvider>::get(&node1_id)
            .expect("internal error: node not found");
        assert_eq!(node1.value().expect("external node has value"), node1_value);
        let total_issuance_actual =
            TotalIssuanceWrap::<GTestTotalIssuanceStorage, GTestTotalIssuanceProvider>::get()
                .expect("internal error: total issuance not found");
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
