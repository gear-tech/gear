// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! Auxiliary implementation of the task pool.

use super::{AuxiliaryDoubleStorageWrap, BlockNumber, DoubleBTreeMap};
use crate::scheduler::{ScheduledTask, TaskPoolImpl};
use gear_core::ids::ProgramId;
use std::cell::RefCell;

/// Task pool implementation that can be used in a native, non-wasm runtimes.
pub type AuxiliaryTaskpool<TaskPoolCallbacks> = TaskPoolImpl<
    TaskPoolStorageWrap,
    ScheduledTask<ProgramId>,
    TaskPoolErrorImpl,
    TaskPoolErrorImpl,
    TaskPoolCallbacks,
>;

std::thread_local! {
    pub(crate) static TASKPOOL_STORAGE: RefCell<DoubleBTreeMap<BlockNumber, ScheduledTask<ProgramId>, ()>> = const { RefCell::new(DoubleBTreeMap::new()) };
}

/// `TaskPool` double storage map manager
pub struct TaskPoolStorageWrap;

impl AuxiliaryDoubleStorageWrap for TaskPoolStorageWrap {
    type Key1 = BlockNumber;
    type Key2 = ScheduledTask<ProgramId>;
    type Value = ();

    fn with_storage<F, R>(f: F) -> R
    where
        F: FnOnce(&DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        TASKPOOL_STORAGE.with_borrow(f)
    }

    fn with_storage_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut DoubleBTreeMap<Self::Key1, Self::Key2, Self::Value>) -> R,
    {
        TASKPOOL_STORAGE.with_borrow_mut(f)
    }
}

/// An implementor of the error returned from calling `TaskPool` trait functions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskPoolErrorImpl {
    /// Occurs when given task already exists in task pool.
    DuplicateTask,
    /// Occurs when task wasn't found in storage.
    TaskNotFound,
}

impl crate::scheduler::TaskPoolError for TaskPoolErrorImpl {
    fn duplicate_task() -> Self {
        Self::DuplicateTask
    }

    fn task_not_found() -> Self {
        Self::TaskNotFound
    }
}
