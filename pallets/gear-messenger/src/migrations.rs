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

use std::marker::PhantomData;
use frame_support::traits::{Get, GetStorageVersion, OnRuntimeUpgrade};
use frame_support::weights::Weight;
use common::storage::{Interval, LinkedNode};
use gear_core::ids::MessageId;
use gear_core::message::{ContextStore, StoredDispatch};
use crate::{Config, Dispatches, Pallet, Waitlist};

pub struct MigrateToV2<T: Config>(PhantomData<T>);

impl<T: Config> MigrateToV2<T> {
    fn migrate_context_store(ctx: v1::ContextStore) -> ContextStore {
        ContextStore {
            outgoing: ctx.outgoing,
            reply: ctx.reply,
            initialized: ctx.initialized,
            reply_sent: ctx.reply_sent,
            reservation_nonce: ctx.reservation_nonce,
            system_reservation: ctx.system_reservation,
        }
    }
}

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    #[cfg(feature = "try-runtime")]
    fn on_runtime_upgrade() -> Weight {
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 2 && onchain == 1 {
            Waitlist::<T>::translate(|_, _, store: (v1::StoredDispatch, Interval<T::BlockNumber>)| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                Some((StoredDispatch {
                    kind: store.0.kind,
                    message: store.0.message,
                    context: store.0.context.map(Self::migrate_context_store),
                }, store.1))
            });

            Dispatches::<T>::translate(|_, store: LinkedNode<MessageId, v1::StoredDispatch>| {
                weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                Some(LinkedNode {
                    next: store.next,
                    value: StoredDispatch {
                        kind: store.value.kind,
                        message: store.value.message,
                        context: store.value.context.map(Self::migrate_context_store),
                    },
                })
            });

            weight = weight.saturating_add(T::DbWeight::get().writes(1));
            current.put::<Pallet<T>>();

            log::info!("Successfully migrated storage");
        } else {
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        weight
    }
}

mod v1 {
    use std::collections::{BTreeMap, BTreeSet};
    use std::marker::PhantomData;
    use frame_support::{codec::{Decode, Encode}, Identity, scale_info::{TypeInfo}};
    use frame_support::pallet_prelude::{CountedStorageMap, StorageDoubleMap};
    use frame_support::storage::types::CountedStorageMapInstance;
    use frame_support::traits::{PalletInfo, StorageInstance};
    use common::storage::{Interval, LinkedNode};
    use gear_core::ids::{MessageId, ProgramId};
    use gear_core::message::{DispatchKind, Payload, StoredMessage};
    use gear_core::reservation::ReservationNonce;
    use crate::{Config, Pallet};

    #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
    pub struct StoredDispatch {
        pub kind: DispatchKind,
        pub message: StoredMessage,
        pub context: Option<ContextStore>,
    }

    #[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
    pub struct ContextStore {
        pub outgoing: BTreeMap<u32, Option<Payload>>,
        pub reply: Option<Payload>,
        pub initialized: BTreeSet<ProgramId>,
        pub awaken: BTreeSet<MessageId>,
        pub reply_sent: bool,
        pub reservation_nonce: ReservationNonce,
        pub system_reservation: Option<u64>,
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
    pub type Dispatches<T> = CountedStorageMap<DispatchesPrefix<T>, Identity, MessageId, LinkedNode<MessageId, StoredDispatch>>;

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
mod tests {
    use frame_support::pallet_prelude::StorageVersion;
    use frame_support::traits::OnRuntimeUpgrade;
    use common::storage::{Interval, LinkedNode};
    use gear_core::ids::{MessageId, ProgramId};
    use gear_core::message::StoredMessage;
    use crate::migrations::{MigrateToV2, v1};
    use crate::mock::*;

    #[test]
    fn migration_to_v2_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(1).put::<GearMessenger>();

            let pid = ProgramId::from(1u64);
            let mid = MessageId::from(2u64);
            let pid2 = ProgramId::from(3u64);

            let dispatch = v1::StoredDispatch {
                kind: Default::default(),
                message: StoredMessage::new(
                    mid,
                    pid,
                    pid2,
                    Default::default(),
                    Default::default(),
                    None
                ),
                context: Some(v1::ContextStore {
                    outgoing: Default::default(),
                    reply: None,
                    initialized: Default::default(),
                    awaken: Default::default(),
                    reply_sent: false,
                    reservation_nonce: Default::default(),
                    system_reservation: None,
                }),
            };

            v1::Waitlist::<Test>::insert(pid, mid, (dispatch.clone(), Interval {
                start: 0,
                finish: 1,
            }));

            v1::Dispatches::<Test>::insert(mid, LinkedNode {
                next: None,
                value: dispatch,
            });

            let _weight = MigrateToV2::<Test>::on_runtime_upgrade();

            assert_eq!(StorageVersion::get::<GearMessenger>(), 2);
        });
    }
}