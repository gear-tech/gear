// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

use crate::{Config, DispatchStash, Dispatches, Pallet, Waitlist};
use common::storage::{Interval, LinkedNode};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use gear_core::{ids::MessageId, message::StoredDelayedDispatch};
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use {
    frame_support::codec::{Decode, Encode},
    sp_std::vec::Vec,
};

pub struct MigrateToV3<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV3<T> {
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 3 && onchain == 2 {
            Waitlist::<T>::translate(
                |_, _, store: (v2::StoredDispatch, Interval<T::BlockNumber>)| {
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                    Some((store.0.into(), store.1))
                },
            );

            Dispatches::<T>::translate(|_, store: LinkedNode<MessageId, v2::StoredDispatch>| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                Some(LinkedNode {
                    next: store.next,
                    value: store.value.into(),
                })
            });

            DispatchStash::<T>::translate(
                |_, store: (v2::StoredDispatch, Interval<T::BlockNumber>)| {
                    if store.0.context.is_some() {
                        log::error!("Previous context on StoredDispatch in DispatchStash should always be None, but was {:?}", store.0.context);
                    }
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                    Some((
                        StoredDelayedDispatch::new(store.0.kind, store.0.message),
                        store.1,
                    ))
                },
            );

            weight = weight.saturating_add(T::DbWeight::get().writes(1));
            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }

    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        let mut count = v2::Waitlist::<T>::iter().count();
        count += v2::Dispatches::<T>::iter().count();
        count += v2::DispatchStash::<T>::iter().count();

        Ok((count as u64).encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        let mut count = Waitlist::<T>::iter().count();
        count += Dispatches::<T>::iter().count();
        count += DispatchStash::<T>::iter().count();

        let old_count: u64 =
            Decode::decode(&mut &state[..]).expect("pre_upgrade provides a valid state; qed");
        assert_eq!(count as u64, old_count);

        Ok(())
    }
}

mod v2 {
    use crate::{Config, Pallet};
    #[cfg(feature = "try-runtime")]
    use common::storage::{Interval, LinkedNode};
    use frame_support::{
        codec::{Decode, Encode},
        scale_info::TypeInfo,
        storage::types::CountedStorageMapInstance,
        traits::{PalletInfo, StorageInstance},
    };
    #[cfg(feature = "try-runtime")]
    use frame_support::{
        pallet_prelude::{CountedStorageMap, StorageDoubleMap, StorageMap},
        Identity,
    };
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{DispatchKind, Payload, StoredMessage},
        reservation::ReservationNonce,
    };
    use sp_std::{
        collections::{btree_map::BTreeMap, btree_set::BTreeSet},
        marker::PhantomData,
    };

    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
    pub struct StoredDispatch {
        pub kind: DispatchKind,
        pub message: StoredMessage,
        pub context: Option<ContextStore>,
    }

    impl From<StoredDispatch> for gear_core::message::StoredDispatch {
        fn from(value: StoredDispatch) -> Self {
            Self::new(value.kind, value.message, value.context.map(Into::into))
        }
    }

    #[derive(
        Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo,
    )]
    pub struct ContextStore {
        pub outgoing: BTreeMap<u32, Option<Payload>>,
        pub reply: Option<Payload>,
        pub initialized: BTreeSet<ProgramId>,
        pub awaken: BTreeSet<MessageId>,
        pub reply_sent: bool,
        pub reservation_nonce: ReservationNonce,
        pub system_reservation: Option<u64>,
    }

    impl From<ContextStore> for gear_core::message::ContextStore {
        fn from(value: ContextStore) -> Self {
            Self::new(
                value.outgoing,
                value.reply,
                value.initialized,
                value.reservation_nonce,
                value.system_reservation,
            )
        }
    }

    pub struct DispatchesPrefix<T: Config>(PhantomData<T>);

    impl<T: Config> StorageInstance for DispatchesPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
        const STORAGE_PREFIX: &'static str = "Dispatches";
    }

    impl<T: Config> CountedStorageMapInstance for DispatchesPrefix<T> {
        type CounterPrefix = Self;
    }

    #[cfg(feature = "try-runtime")]
    pub type Dispatches<T> = CountedStorageMap<
        DispatchesPrefix<T>,
        Identity,
        MessageId,
        LinkedNode<MessageId, StoredDispatch>,
    >;

    pub struct DispatchStashPrefix<T: Config>(PhantomData<T>);

    impl<T: Config> StorageInstance for DispatchStashPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
        const STORAGE_PREFIX: &'static str = "DispatchStash";
    }

    #[cfg(feature = "try-runtime")]
    #[allow(type_alias_bounds)]
    pub type DispatchStash<T: Config> = StorageMap<
        DispatchStashPrefix<T>,
        Identity,
        MessageId,
        (StoredDispatch, Interval<T::BlockNumber>),
    >;

    pub struct WaitlistPrefix<T: Config>(PhantomData<T>);

    impl<T: Config> StorageInstance for WaitlistPrefix<T> {
        fn pallet_prefix() -> &'static str {
            <<T as frame_system::Config>::PalletInfo as PalletInfo>::name::<Pallet<T>>()
                .expect("No name found for the pallet in the runtime!")
        }
        const STORAGE_PREFIX: &'static str = "Waitlist";
    }

    #[cfg(feature = "try-runtime")]
    #[allow(type_alias_bounds)]
    pub type Waitlist<T: Config> = StorageDoubleMap<
        WaitlistPrefix<T>,
        Identity,
        ProgramId,
        Identity,
        MessageId,
        (StoredDispatch, Interval<T::BlockNumber>),
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod test {
    use crate::{
        migrations::{v2, MigrateToV3},
        mock::*,
        DispatchStash, Dispatches, Waitlist,
    };
    use common::storage::{Interval, LinkedNode};
    use frame_support::{pallet_prelude::StorageVersion, traits::OnRuntimeUpgrade};
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{StoredDelayedDispatch, StoredMessage},
    };

    #[test]
    fn migration_to_v3_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(2).put::<GearMessenger>();

            let pid = ProgramId::from(1u64);
            let mid = MessageId::from(2u64);
            let pid2 = ProgramId::from(3u64);

            let dispatch = v2::StoredDispatch {
                kind: Default::default(),
                message: StoredMessage::new(
                    mid,
                    pid,
                    pid2,
                    Default::default(),
                    Default::default(),
                    None,
                ),
                context: Some(v2::ContextStore {
                    outgoing: Default::default(),
                    reply: None,
                    initialized: Default::default(),
                    awaken: Default::default(),
                    reply_sent: false,
                    reservation_nonce: Default::default(),
                    system_reservation: None,
                }),
            };

            let dispatch2 = v2::StoredDispatch {
                kind: Default::default(),
                message: StoredMessage::new(mid, pid2, pid, Default::default(), 100, None),
                context: Some(v2::ContextStore {
                    outgoing: Default::default(),
                    reply: None,
                    initialized: Default::default(),
                    awaken: Default::default(),
                    reply_sent: true,
                    reservation_nonce: Default::default(),
                    system_reservation: Some(1_000_000_000),
                }),
            };

            let dispatch3 = v2::StoredDispatch {
                kind: Default::default(),
                message: StoredMessage::new(mid, pid, pid2, Default::default(), 1_000_000, None),
                context: None,
            };

            v2::Waitlist::<Test>::insert(
                pid,
                mid,
                (
                    dispatch.clone(),
                    Interval {
                        start: 0,
                        finish: 1,
                    },
                ),
            );

            v2::Dispatches::<Test>::insert(
                mid,
                LinkedNode {
                    next: None,
                    value: dispatch2.clone(),
                },
            );

            v2::DispatchStash::<Test>::insert(
                mid,
                (
                    dispatch3.clone(),
                    Interval {
                        start: 0,
                        finish: 1,
                    },
                ),
            );

            let state = MigrateToV3::<Test>::pre_upgrade().unwrap();
            let _ = MigrateToV3::<Test>::on_runtime_upgrade();
            MigrateToV3::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearMessenger>(), 3);

            assert_eq!(
                Waitlist::<Test>::get(pid, mid)
                    .expect("Waitlist failed to migrate.")
                    .0,
                dispatch.into()
            );

            assert_eq!(
                Dispatches::<Test>::get(mid)
                    .expect("Waitlist failed to migrate.")
                    .value,
                dispatch2.into()
            );

            assert_eq!(
                DispatchStash::<Test>::get(mid)
                    .expect("Waitlist failed to migrate.")
                    .0,
                StoredDelayedDispatch::new(dispatch3.kind, dispatch3.message)
            );
        });
    }
}