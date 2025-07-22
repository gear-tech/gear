// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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

//! Auxiliary (for tests) task pool implementation for the crate.

use crate::{constants::BlockNumber, state::WithOverlay};
use gear_common::{
    scheduler::{TaskPool, TaskPoolCallbacks, TaskPoolError, TaskPoolImpl},
    storage::{AuxiliaryDoubleStorageWrap, DoubleBTreeMap, KeyIterableByKeyMap},
};
use gear_core::{ids::ActorId, tasks::VaraScheduledTask};
use std::thread::LocalKey;

type AuxiliaryTaskpool = TaskPoolImpl<
    TaskPoolStorageWrap,
    VaraScheduledTask<ActorId>,
    TaskPoolErrorImpl,
    TaskPoolErrorImpl,
    TaskPoolCallbacksImpl,
>;

pub(crate) type TaskPoolStorage =
    WithOverlay<DoubleBTreeMap<BlockNumber, VaraScheduledTask<ActorId>, ()>>;
std::thread_local! {
    pub(crate) static TASKPOOL_STORAGE: TaskPoolStorage = Default::default();
}

fn storage() -> &'static LocalKey<TaskPoolStorage> {
    &TASKPOOL_STORAGE
}

#[derive(Debug, Default)]
pub(crate) struct TaskPoolManager;

impl TaskPoolManager {
    /// Adapted by argument types version of the task pool `add` method.
    pub(crate) fn add(
        &self,
        block_number: BlockNumber,
        task: VaraScheduledTask<ActorId>,
    ) -> Result<(), TaskPoolErrorImpl> {
        <AuxiliaryTaskpool as TaskPool>::add(block_number, task)
    }

    /// Adapted by argument types version of the task pool `clear` method.
    pub(crate) fn clear(&self) {
        <AuxiliaryTaskpool as TaskPool>::clear();
    }

    /// Adapted by argument types version of the task pool `contains` method.
    #[allow(unused)]
    pub(crate) fn contains(
        &self,
        block_number: &BlockNumber,
        task: &VaraScheduledTask<ActorId>,
    ) -> bool {
        <AuxiliaryTaskpool as TaskPool>::contains(block_number, task)
    }

    /// Adapted by argument types version of the task pool `delete` method.
    pub(crate) fn delete(
        &self,
        block_number: BlockNumber,
        task: VaraScheduledTask<ActorId>,
    ) -> Result<(), TaskPoolErrorImpl> {
        <AuxiliaryTaskpool as TaskPool>::delete(block_number, task)
    }

    /// Adapted by argument types version of the task pool `drain_prefix_keys`
    /// method.
    pub(crate) fn drain_prefix_keys(
        &self,
        block_number: BlockNumber,
    ) -> <TaskPoolStorageWrap as KeyIterableByKeyMap>::DrainIter {
        AuxiliaryTaskpool::drain_prefix_keys(block_number)
    }
}

/// Task pool callbacks implementor.
pub(crate) struct TaskPoolCallbacksImpl;

impl TaskPoolCallbacks for TaskPoolCallbacksImpl {
    type OnAdd = ();
    type OnDelete = ();
}

/// `TaskPool` double storage map manager
pub struct TaskPoolStorageWrap;

impl AuxiliaryDoubleStorageWrap for TaskPoolStorageWrap {
    type Key1 = BlockNumber;
    type Key2 = VaraScheduledTask<ActorId>;
    type Value = ();

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|tps| f(&tps.data()))
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        storage().with(|tps| f(&mut tps.data_mut()))
    }
}

/// An implementor of the error returned from calling `TaskPool` trait
/// functions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskPoolErrorImpl {
    /// Occurs when given task already exists in task pool.
    DuplicateTask,
    /// Occurs when task wasn't found in storage.
    TaskNotFound,
}

impl TaskPoolError for TaskPoolErrorImpl {
    fn duplicate_task() -> Self {
        Self::DuplicateTask
    }

    fn task_not_found() -> Self {
        Self::TaskNotFound
    }
}

#[cfg(test)]
mod tests {
    use super::TaskPoolManager;
    use gear_core::{ids::ActorId, tasks::VaraScheduledTask};

    #[test]
    fn test_taskpool() {
        let manager = TaskPoolManager;

        let block_1_tasks = [
            VaraScheduledTask::<ActorId>::SendDispatch(42.into()),
            VaraScheduledTask::<ActorId>::SendUserMessage {
                message_id: 422.into(),
                to_mailbox: true,
            },
        ];
        let block_2_tasks = [
            VaraScheduledTask::<ActorId>::RemoveGasReservation(922.into(), 1.into()),
            VaraScheduledTask::<ActorId>::RemoveFromWaitlist(42.into(), 44.into()),
        ];

        block_1_tasks
            .iter()
            .for_each(|task| manager.add(1, task.clone()).unwrap());

        block_2_tasks
            .iter()
            .for_each(|task| manager.add(2, task.clone()).unwrap());

        for task in block_1_tasks.iter() {
            assert!(manager.contains(&1, task));
        }

        for task in block_2_tasks.iter() {
            assert!(manager.contains(&2, task));
        }

        for task in manager.drain_prefix_keys(1) {
            assert!(
                block_1_tasks.contains(&task),
                "task not found in block 1 tasks"
            );
        }

        for task in manager.drain_prefix_keys(2) {
            assert!(
                block_2_tasks.contains(&task),
                "task not found in block 2 tasks"
            );
        }

        for task in block_1_tasks.iter() {
            assert!(!manager.contains(&1, task));
        }

        for task in block_2_tasks.iter() {
            assert!(!manager.contains(&2, task));
        }

        let task = VaraScheduledTask::<ActorId>::RemoveFromMailbox(422.into(), 16.into());
        manager.add(3, task.clone()).unwrap();
        manager.add(4, task.clone()).unwrap();
        manager.delete(4, task.clone()).unwrap();
        manager.clear();

        assert!(!manager.contains(&3, &task));
        assert!(!manager.contains(&4, &task));
    }
}
