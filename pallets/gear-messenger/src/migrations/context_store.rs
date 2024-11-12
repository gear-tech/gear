// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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


use core::marker::PhantomData;
use crate::{Config, Pallet, Waitlist};
use common::{
    storage::{Interval, LinkedNode},
    MessageId,
};

#[cfg(feature = "try-runtime")]
use {
    frame_support::ensure,
    sp_runtime::{
        codec::{Decode, Encode},
        TryRuntimeError,
    },
    sp_std::vec::Vec,
};

use frame_support::{
    traits::{GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
    weights::Weight,
};
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::message::{ContextStore, StoredDispatch};

use crate::Dispatches;
use sp_runtime::traits::Get;
pub struct RemoveCommitStorage<T: Config>(PhantomData<T>);

const MIGRATE_FROM_VERSION: u16 = 3;
const MIGRATE_TO_VERSION: u16 = 4;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 4;

fn translate_dispatch(dispatch: v3::StoredDispatch) -> StoredDispatch {
    StoredDispatch::new(
        dispatch.kind,
        dispatch.message,
        dispatch.context.map(|ctx| {
            ContextStore::new(
                ctx.initialized,
                ctx.reservation_nonce,
                ctx.system_reservation,
                // set local_none to previous number of outgoing messages to not break existing IDs.
                ctx.outgoing.len() as u32,
            )
        }),
    )
}

impl<T: Config> OnRuntimeUpgrade for RemoveCommitStorage<T> {
    fn on_runtime_upgrade() -> Weight {
        let onchain = Pallet::<T>::on_chain_storage_version();

        let mut weight = T::DbWeight::get().reads(1);
        let mut counter = 0;

        if onchain == MIGRATE_FROM_VERSION {
            let current = Pallet::<T>::in_code_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            Dispatches::<T>::translate(|_, value: LinkedNode<MessageId, v3::StoredDispatch>| {
                counter += 1;
                Some(LinkedNode {
                    next: value.next,
                    value: translate_dispatch(value.value),
                })
            });

            Waitlist::<T>::translate(
                |_, _, (dispatch, interval): (v3::StoredDispatch, Interval<BlockNumberFor<T>>)| {
                    counter += 1;
                    Some((translate_dispatch(dispatch), interval))
                },
            );
            // each `translate` call results in read to DB to fetch dispatch and then write to DB to update it.
            weight = weight.saturating_add(T::DbWeight::get().reads_writes(counter, counter));
            weight = weight.saturating_add(T::DbWeight::get().writes(1));

            update_to.put::<Pallet<T>>();

            log::info!("✅ Successfully migrated storage. {counter} codes have been migrated");
        } else {
            log::info!("🟠 Migration requires onchain version {MIGRATE_FROM_VERSION}, so was skipped for {onchain:?}");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        let current = Pallet::<T>::in_code_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        if onchain == MIGRATE_FROM_VERSION {
            ensure!(
                current == ALLOWED_CURRENT_STORAGE_VERSION,
                "Current storage version is not allowed for migration, check migration code in order to allow it."
            );

            let expected_waitlist = Waitlist::<T>::iter().count() as u64;
            let expected_dispatches = Dispatches::<T>::iter().count() as u64;

            Ok(Some(expected_waitlist.saturating_add(expected_dispatches)).encode())
        } else {
            Ok(Option::<u64>::None.encode())
        }
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
        if let Some(expected_count) = Option::<u64>::decode(&mut state.as_ref())
            .map_err(|_| "`pre_upgrade` provided an invalid state")?
        {
            let current_waitlist = Waitlist::<T>::iter().count() as u64;
            let current_dispatches = Dispatches::<T>::iter().count() as u64;

            ensure!(
                expected_count == current_waitlist.saturating_add(current_dispatches),
                "Number of waitlist or dispatch entries is different after migration"
            );
            ensure!(
                Pallet::<T>::on_chain_storage_version() == MIGRATE_TO_VERSION,
                "incorrect storage version after migration"
            );
        }

        Ok(())
    }
}

mod v3 {
    use common::ProgramId;

    use gear_core::{
        message::{DispatchKind, Payload, StoredMessage},
        reservation::ReservationNonce,
    };

    use scale_info::{
        scale::{Decode, Encode},
        TypeInfo,
    };

    use sp_std::collections::{btree_map::BTreeMap, btree_set::BTreeSet};

    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
    pub struct StoredDispatch {
        pub kind: DispatchKind,
        pub message: StoredMessage,
        pub context: Option<ContextStore>,
    }
    #[derive(
        Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
    )]
    pub struct ContextStore {
        pub outgoing: BTreeMap<u32, Option<Payload>>,
        pub reply: Option<Payload>,
        pub initialized: BTreeSet<ProgramId>,
        pub reservation_nonce: ReservationNonce,
        pub system_reservation: Option<u64>,
    }
}
