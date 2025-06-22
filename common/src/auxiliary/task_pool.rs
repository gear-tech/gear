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

//! Auxiliary implementation of the task pool.

use super::{AuxiliaryDoubleStorageWrap, BlockNumber, DoubleBTreeMap};
use crate::{auxiliary::overlay::WithOverlay, scheduler::TaskPoolImpl};
use gear_core::{ids::ActorId, tasks::VaraScheduledTask};
use std::thread::LocalKey;

/// Task pool implementation that can be used in a native, non-wasm runtimes.
pub type AuxiliaryTaskpool<TaskPoolCallbacks> = TaskPoolImpl<
    TaskPoolStorageWrap,
    VaraScheduledTask<ActorId>,
    TaskPoolErrorImpl,
    TaskPoolErrorImpl,
    TaskPoolCallbacks,
>;

pub(crate) type TaskPoolStorage =
    WithOverlay<DoubleBTreeMap<BlockNumber, VaraScheduledTask<ActorId>, ()>>;
std::thread_local! {
    pub(crate) static TASKPOOL_STORAGE: TaskPoolStorage = Default::default();
}

fn storage() -> &'static LocalKey<TaskPoolStorage> {
    &TASKPOOL_STORAGE
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
