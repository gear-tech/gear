// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! # Gear Scheduler Pallet

#![cfg_attr(not(feature = "std"), no_std)]

// Database migration module.
pub mod migration;

// Runtime mock for running tests.
#[cfg(test)]
mod mock;

// Unit tests module.
#[cfg(test)]
mod tests;

// Public exports from pallet.
pub use pallet::*;

// Gear Scheduler Pallet module.
#[frame_support::pallet]
pub mod pallet {
    pub use frame_support::weights::Weight;

    use common::{
        scheduler::{SchedulingCostsPerBlock, TaskPoolImpl, *},
        storage::*,
        BlockLimiter, Origin,
    };
    use frame_support::{
        dispatch::DispatchError,
        pallet_prelude::*,
        storage::PrefixIterator,
        traits::{Get, StorageVersion},
    };
    use frame_system::pallet_prelude::*;
    use sp_std::{collections::btree_set::BTreeSet, convert::TryInto, marker::PhantomData};

    pub type Cost = u64;

    pub(crate) type GasAllowanceOf<T> = <<T as Config>::BlockLimiter as BlockLimiter>::GasAllowance;

    /// The current storage version.
    const SCHEDULER_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    // Gear Scheduler Pallet's `Config`.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Block limits.
        type BlockLimiter: BlockLimiter<Balance = u64>;

        /// Amount of blocks for extra delay used to secure from outdated tasks.
        #[pallet::constant]
        type ReserveThreshold: Get<Self::BlockNumber>;

        /// Cost for storing in waitlist per block.
        #[pallet::constant]
        type WaitlistCost: Get<Cost>;

        /// Cost for storing in mailbox per block.
        #[pallet::constant]
        type MailboxCost: Get<Cost>;

        /// Cost for reservation holding.
        #[pallet::constant]
        type ReservationCost: Get<Cost>;

        /// Cost for reservation holding.
        #[pallet::constant]
        type DispatchHoldCost: Get<Cost>;
    }

    // Gear Scheduler Pallet itself.
    //
    // Uses without storage info to avoid direct access to pallet's
    // storage from outside.
    //
    // Uses `SCHEDULER_STORAGE_VERSION` as current storage version.
    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(SCHEDULER_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Gear Scheduler Pallet error type.
    //
    // Used as inner error type for `Scheduler` implementation.
    #[pallet::error]
    pub enum Error<T> {
        /// Occurs when given task already exists in task pool.
        DuplicateTask,
        /// Occurs when task wasn't found in storage.
        TaskNotFound,
    }

    // Implementation of `DequeueError` for `Error<T>`
    // usage as `Queue::Error`.
    impl<T: crate::Config> TaskPoolError for Error<T> {
        fn duplicate_task() -> Self {
            Self::DuplicateTask
        }

        fn task_not_found() -> Self {
            Self::TaskNotFound
        }
    }

    /// Account Id type of the task.
    type AccountId<T> = <T as frame_system::Config>::AccountId;

    /// Task type of the scheduler.
    type Task<T> = ScheduledTask<AccountId<T>>;

    // BTreeSet used to exclude duplicates and always keep collection sorted.
    /// Missed blocks collection type.
    ///
    /// Defines block number, which should already contain no tasks,
    /// because they were processed before.
    /// Missed blocks processing prioritized.
    type MissedBlocksCollection<T> = BTreeSet<BlockNumberFor<T>>;

    // Below goes storages and their gear's wrapper implementations.
    //
    // Note, that we declare storages private to avoid outside
    // interaction with them, but wrappers - public to be able
    // use them as generic parameters in public `Scheduler`
    // implementation.

    // ----

    // Private storage for missed blocks collection.
    #[pallet::storage]
    type MissedBlocks<T> = StorageValue<_, MissedBlocksCollection<T>>;

    // Public wrap of the missed blocks collection.
    common::wrap_storage_value!(
        storage: MissedBlocks,
        name: MissedBlocksWrap,
        value: MissedBlocksCollection<T>
    );

    // ----

    // Private storage for task pool elements.
    // Primary item stored as second key of double map for optimization.
    // Value here is useless, so unit type used as space saver:
    // `assert_eq!(().encode().len(), 0)`
    #[pallet::storage]
    type TaskPool<T: Config> =
        StorageDoubleMap<_, Identity, BlockNumberFor<T>, Identity, Task<T>, ()>;

    // Public wrap of the mailbox elements.
    common::wrap_extended_storage_double_map!(
        storage: TaskPool,
        name: TaskPoolWrap,
        key1: BlockNumberFor<T>,
        key2: Task<T>,
        value: (),
        length: usize
    );

    // ----

    // Below goes callbacks, used for task scope algorithm.
    //
    // Note, that they are public like storage wrappers
    // only to be able to use as public trait's generics.

    // ----

    /// Callback function for success `add` and `delete` actions.
    pub struct OnChange<T: crate::Config>(PhantomData<T>);

    // Callback trait implementation.
    //
    // Addition to or deletion from task scope represented with single DB write.
    // This callback reduces block gas allowance by that value.
    impl<T: crate::Config> EmptyCallback for OnChange<T> {
        fn call() {
            let weight = T::DbWeight::get().writes(1);
            log::debug!(
                "TaskPool::OnChange; weight = {weight}, GasAllowance = {}",
                GasAllowanceOf::<T>::get()
            );
            GasAllowanceOf::<T>::decrease(weight.ref_time());
        }
    }

    // ----

    /// Store of queue action's callbacks.
    pub struct TaskPoolCallbacksImpl<T: crate::Config>(PhantomData<T>);

    // Callbacks store for task pool trait implementation.
    impl<T: crate::Config> TaskPoolCallbacks for TaskPoolCallbacksImpl<T> {
        type OnAdd = OnChange<T>;
        type OnDelete = OnChange<T>;
    }

    // ----

    // Below goes costs implementation.

    impl<T: crate::Config> SchedulingCostsPerBlock for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type BlockNumber = BlockNumberFor<T>;
        type Cost = Cost;

        fn reserve_for() -> Self::BlockNumber {
            T::ReserveThreshold::get()
        }

        fn code() -> Self::Cost {
            todo!("#646");
        }

        fn mailbox() -> Self::Cost {
            T::MailboxCost::get()
        }

        fn program() -> Self::Cost {
            todo!("#646");
        }

        fn waitlist() -> Self::Cost {
            T::WaitlistCost::get()
        }

        fn reservation() -> Self::Cost {
            T::ReservationCost::get()
        }

        fn dispatch_stash() -> Self::Cost {
            T::DispatchHoldCost::get()
        }
    }

    // Below goes final `Scheduler` implementation for
    // Gear Scheduler Pallet based on above generated
    // types and parameters.

    /// Delayed tasks centralized behavior for
    /// Gear Scheduler Pallet.
    ///
    /// See `gear_common::scheduler::Scheduler` for
    /// complete documentation.
    impl<T: crate::Config> Scheduler for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type BlockNumber = BlockNumberFor<T>;
        type Task = Task<T>;
        type Cost = u64;
        type MissedBlocksCollection = MissedBlocksCollection<T>;
        type Error = Error<T>;
        type OutputError = DispatchError;

        type CostsPerBlock = Self;

        type MissedBlocks = MissedBlocksWrap<T>;

        type TaskPool = TaskPoolImpl<
            TaskPoolWrap<T>,
            Self::Task,
            Self::Error,
            DispatchError,
            TaskPoolCallbacksImpl<T>,
        >;
    }
}
