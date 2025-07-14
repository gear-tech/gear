// This file is part of Gear.

// Copyright (C) 2023-2025 Gear Technologies Inc.
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

use crate::{pallet::TaskPool, Config, Pallet};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::tasks::VaraScheduledTask;
use sp_std::{collections::btree_map::BTreeMap, marker::PhantomData};

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
};

const MIGRATE_FROM_VERSION: u16 = 2;
const MIGRATE_TO_VERSION: u16 = 3;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 3;

pub struct MigrateRemoveProgramPauseTasks<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateRemoveProgramPauseTasks<T> {
    fn on_runtime_upgrade() -> Weight {
        // 1 read for onchain storage version
        let mut weight = T::DbWeight::get().reads(1);

        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::in_code_storage_version();

            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);

            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            let mut total_counter = 0;
            let mut removed_tasks = 0;

            let mut new_tasks: BTreeMap<BlockNumberFor<T>, VaraScheduledTask<T::AccountId>> =
                BTreeMap::new();

            v2::TaskPool::<T>::drain().for_each(|(block_number, task, _)| {
                // We need to read the task from storage, and then write it back in the new format.
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                match task {
                    v2::VaraScheduledTask::PauseProgram(_)
                    | v2::VaraScheduledTask::RemoveCode(_)
                    | v2::VaraScheduledTask::RemovePausedProgram(_)
                    | v2::VaraScheduledTask::RemoveResumeSession(_) => {
                        removed_tasks += 1;
                    }
                    // We need to convert the task to the new format.
                    v2::VaraScheduledTask::RemoveFromMailbox(rfm, message_id) => {
                        new_tasks.insert(
                            block_number,
                            VaraScheduledTask::RemoveFromMailbox(rfm, message_id),
                        );
                    }
                    v2::VaraScheduledTask::RemoveFromWaitlist(actor_id, message_id) => {
                        new_tasks.insert(
                            block_number,
                            VaraScheduledTask::RemoveFromWaitlist(actor_id, message_id),
                        );
                    }
                    v2::VaraScheduledTask::WakeMessage(actor_id, message_id) => {
                        new_tasks.insert(
                            block_number,
                            VaraScheduledTask::WakeMessage(actor_id, message_id),
                        );
                    }
                    v2::VaraScheduledTask::SendDispatch(sd) => {
                        new_tasks.insert(block_number, VaraScheduledTask::SendDispatch(sd));
                    }
                    v2::VaraScheduledTask::SendUserMessage {
                        message_id,
                        to_mailbox,
                    } => {
                        new_tasks.insert(
                            block_number,
                            VaraScheduledTask::SendUserMessage {
                                message_id,
                                to_mailbox,
                            },
                        );
                    }
                    v2::VaraScheduledTask::RemoveGasReservation(actor_id, reservation_id) => {
                        new_tasks.insert(
                            block_number,
                            VaraScheduledTask::RemoveGasReservation(actor_id, reservation_id),
                        );
                    }
                }

                total_counter += 1;
            });

            new_tasks.into_iter().for_each(|(block_number, task)| {
                TaskPool::<T>::insert(block_number, task, ());
            });

            // Put new storage version
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {total_counter} tasks were traversed, {removed_tasks} tasks were removed.");
        } else {
            log::info!("🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        let res = if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            Some(v2::TaskPool::<T>::iter().count() as u64)
        } else {
            None
        };

        Ok(res.encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if let Some(old_count) = Option::<u64>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            let new_count = TaskPool::<T>::iter_keys().count() as u64;
            ensure!(old_count >= new_count, "incorrect count of elements");
        }

        Ok(())
    }
}

mod v2 {
    use common::{ActorId, MessageId, ReservationId};
    use frame_support::Identity;
    use gear_core::ids::CodeId;
    use parity_scale_codec::MaxEncodedLen;
    use sp_runtime::{
        codec::{Decode, Encode},
        scale_info::TypeInfo,
    };
    use sp_std::prelude::*;

    #[derive(
        Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Encode, Decode, TypeInfo, MaxEncodedLen,
    )]
    pub enum ScheduledTask<RFM, SD, SUM> {
        #[codec(index = 0)]
        PauseProgram(ActorId),
        #[codec(index = 1)]
        RemoveCode(CodeId),
        #[codec(index = 2)]
        RemoveFromMailbox(RFM, MessageId),
        #[codec(index = 3)]
        RemoveFromWaitlist(ActorId, MessageId),
        #[codec(index = 4)]
        RemovePausedProgram(ActorId),
        #[codec(index = 5)]
        WakeMessage(ActorId, MessageId),
        #[codec(index = 6)]
        SendDispatch(SD),
        #[codec(index = 7)]
        SendUserMessage {
            message_id: MessageId,
            to_mailbox: SUM,
        },
        #[codec(index = 8)]
        RemoveGasReservation(ActorId, ReservationId),
        #[codec(index = 9)]
        RemoveResumeSession(u32),
    }

    pub type VaraScheduledTask<AccountId> = ScheduledTask<AccountId, MessageId, bool>;

    use crate::{Config, Pallet};
    use frame_system::pallet_prelude::BlockNumberFor;

    #[frame_support::storage_alias]
    pub type TaskPool<T: Config> = StorageDoubleMap<
        Pallet<T>,
        Identity,
        BlockNumberFor<T>,
        Identity,
        VaraScheduledTask<<T as frame_system::Config>::AccountId>,
        (),
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use super::*;
    use crate::mock::*;
    use frame_support::traits::StorageVersion;
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::tasks::VaraScheduledTask;
    use sp_runtime::traits::Zero;

    #[test]
    fn v3_remove_program_pause_tasks_migration_works() {
        let _ = tracing_subscriber::fmt::try_init();

        new_test_ext().execute_with(|| {
            StorageVersion::new(MIGRATE_FROM_VERSION).put::<GearScheduler>();

            // Insert some old tasks into the task pool that will be removed during migration
            v2::TaskPool::<Test>::insert(
                BlockNumberFor::<Test>::from(0u64),
                v2::VaraScheduledTask::PauseProgram(0u64.into()),
                (),
            );
            v2::TaskPool::<Test>::insert(
                BlockNumberFor::<Test>::from(0u64),
                v2::VaraScheduledTask::RemoveCode([0u8; 32].into()),
                (),
            );
            v2::TaskPool::<Test>::insert(
                BlockNumberFor::<Test>::from(0u64),
                v2::VaraScheduledTask::RemovePausedProgram(0u64.into()),
                (),
            );
            v2::TaskPool::<Test>::insert(
                BlockNumberFor::<Test>::from(0u64),
                v2::VaraScheduledTask::RemoveResumeSession(0u32),
                (),
            );

            // Insert some tasks that will remain after migration
            v2::TaskPool::<Test>::insert(
                BlockNumberFor::<Test>::from(0u64),
                v2::VaraScheduledTask::RemoveFromWaitlist(0u64.into(), [0u8; 32].into()),
                (),
            );

            let state = MigrateRemoveProgramPauseTasks::<Test>::pre_upgrade().unwrap();
            let w = MigrateRemoveProgramPauseTasks::<Test>::on_runtime_upgrade();
            assert!(!w.is_zero());
            MigrateRemoveProgramPauseTasks::<Test>::post_upgrade(state).unwrap();

            assert_eq!(TaskPool::<Test>::iter().count(), 1);

            TaskPool::<Test>::iter_keys().for_each(|(bn, task)| {
                assert_eq!(bn, BlockNumberFor::<Test>::from(0u64));
                assert_eq!(
                    task,
                    VaraScheduledTask::RemoveFromWaitlist(0u64.into(), [0u8; 32].into()),
                );
            });

            assert_eq!(StorageVersion::get::<GearScheduler>(), MIGRATE_TO_VERSION);
        })
    }
}
