// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
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

use crate::{Config, Pallet, QueueOf};
use common::{
    event::{MessageWokenSystemReason, SystemReason},
    storage::Queue,
    Origin,
};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use gear_core::ids::MessageId;
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    common::storage::IterableMap,
    gear_core::ids::ProgramId,
    parity_scale_codec::{Decode, Encode},
    sp_runtime::TryRuntimeError,
    sp_std::vec::Vec,
};

pub struct MigrateWaitingInitList<T>(PhantomData<T>);

impl<T> OnRuntimeUpgrade for MigrateWaitingInitList<T>
where
    T: Config + pallet_gear_program::Config,
    T::AccountId: Origin,
{
    fn on_runtime_upgrade() -> Weight {
        let current = pallet_gear_program::Pallet::<T>::current_storage_version();
        let onchain = pallet_gear_program::Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for the on-chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 4 && onchain == 3 {
            waiting_init_list::WaitingInitStorage::<T>::translate(
                |program_id, messages: Vec<MessageId>| {
                    // read and remove an element
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));

                    for message_id in messages {
                        if let Some(dispatch) = Pallet::<T>::wake_dispatch(
                            program_id,
                            message_id,
                            MessageWokenSystemReason::WaitInitListMigration.into_reason(),
                        ) {
                            // remove from waitlist
                            weight = weight.saturating_add(T::DbWeight::get().writes(1));

                            QueueOf::<T>::queue(dispatch).unwrap_or_else(|e| {
                                unreachable!("Message queue corrupted! {:?}", e)
                            });

                            // push to queue
                            weight = weight.saturating_add(T::DbWeight::get().writes(1));
                        }
                    }

                    None
                },
            );

            current.put::<pallet_gear_program::Pallet<T>>();

            log::info!("Successfully migrated storage");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let init_msgs = waiting_init_list::WaitingInitStorage::<T>::iter().count();
        let queue_msgs = QueueOf::<T>::iter().count();

        Ok((init_msgs as u64, queue_msgs as u64).encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        let (init_msgs, queue_msgs): (u64, u64) =
            Decode::decode(&mut &state[..]).expect("failed to decode the state");

        let current_queue_msgs = QueueOf::<T>::iter().count();
        assert_eq!(init_msgs + queue_msgs, current_queue_msgs as u64);

        Ok(())
    }
}

mod waiting_init_list {
    use super::*;
    use crate::Pallet;
    use frame_support::{pallet_prelude::StorageMap, Identity};
    use gear_core::ids::{MessageId, ProgramId};

    pub type WaitingInitStorage<T> = StorageMap<
        _GeneratedPrefixForStorageWaitingInitStorage<T>,
        Identity,
        ProgramId,
        Vec<MessageId>,
    >;

    #[doc(hidden)]
    pub struct _GeneratedPrefixForStorageWaitingInitStorage<T>(PhantomData<(T,)>);

    impl<T: Config> frame_support::traits::StorageInstance
        for _GeneratedPrefixForStorageWaitingInitStorage<T>
    {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as frame_support::traits::PalletInfo>::name::<Pallet<T>>().expect("No name found for the pallet in the runtime! This usually means that the pallet wasn't added to `construct_runtime!`.")
        }
        const STORAGE_PREFIX: &'static str = "WaitingInitStorage";
    }
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod tests {
    use super::*;
    use crate::{
        mock::{new_test_ext, GearProgram, Test},
        tests::init_logger,
        GasHandlerOf, WaitlistOf,
    };
    use common::{storage::Waitlist, GasTree};
    use frame_support::pallet_prelude::StorageVersion;
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::message::{
        ContextStore, DispatchKind, MessageDetails, Payload, ReplyDetails, SignalDetails,
        StoredDispatch, StoredMessage,
    };
    use gear_core_errors::{ReplyCode, SignalCode, SuccessReplyReason};
    use sp_runtime::traits::Zero;

    #[test]
    fn migration_works() {
        init_logger();

        new_test_ext().execute_with(|| {
            StorageVersion::new(3).put::<GearProgram>();

            let program_id = ProgramId::from(0);
            let message_id = MessageId::from(0);
            let messages = vec![message_id];

            let stored_msg = StoredMessage::new(
                message_id,
                program_id,
                program_id,
                Payload::default(),
                0,
                None,
            );
            let stored_dispatch = StoredDispatch::new(DispatchKind::Init, stored_msg, None);

            let multiplier = <Test as pallet_gear_bank::Config>::GasMultiplier::get();
            GasHandlerOf::<Test>::create(0, multiplier, message_id, 0).unwrap();
            waiting_init_list::WaitingInitStorage::<Test>::insert(program_id, messages);
            WaitlistOf::<Test>::insert(
                stored_dispatch.clone(),
                BlockNumberFor::<Test>::max_value(),
            )
            .unwrap();

            let state = MigrateWaitingInitList::<Test>::pre_upgrade().unwrap();
            let weight = MigrateWaitingInitList::<Test>::on_runtime_upgrade();
            assert!(!weight.is_zero());
            MigrateWaitingInitList::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearProgram>(), 4);
            assert_eq!(
                waiting_init_list::WaitingInitStorage::<Test>::iter().count(),
                0
            );
            assert_eq!(QueueOf::<Test>::dequeue().unwrap(), Some(stored_dispatch));
        });
    }
}
