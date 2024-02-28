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
use frame_system::pallet_prelude::BlockNumberFor;
use gear_core::ids::MessageId;
use sp_std::marker::PhantomData;
#[cfg(feature = "try-runtime")]
use {
    frame_support::{codec::Decode, dispatch::DispatchError},
    parity_scale_codec::Encode,
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
                |_, _, (dispatch, interval): (v2::StoredDispatch, Interval<BlockNumberFor<T>>)| {
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                    Some((dispatch.into(), interval))
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
                |_, store: (v2::StoredDispatch, Interval<BlockNumberFor<T>>)| {
                    if store.0.context.is_some() {
                        log::error!("Previous context on StoredDispatch in DispatchStash should always be None, but was Some for message id {:?}", store.0.message.id());
                    }
                    weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
                    Some((store.0.into(), store.1))
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
    fn pre_upgrade() -> Result<Vec<u8>, DispatchError> {
        let mut count = v2::Waitlist::<T>::iter().count();
        count += v2::Dispatches::<T>::iter().count();
        count += v2::DispatchStash::<T>::iter().inspect(
            |store| {
                if store.1.0.context.is_some() {
                    panic!("Previous context on StoredDispatch in DispatchStash should always be None, but was Some for message id {:?}", store.1.0.message.id());
                }
            },
        ).count();

        Ok((count as u64).encode())
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), DispatchError> {
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
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{DispatchKind, Payload, StoredDelayedDispatch, StoredMessage},
        reservation::ReservationNonce,
    };
    use sp_std::{
        collections::{btree_map::BTreeMap, btree_set::BTreeSet},
        marker::PhantomData,
    };
    #[cfg(feature = "try-runtime")]
    use {
        frame_support::{
            pallet_prelude::{CountedStorageMap, StorageDoubleMap, StorageMap},
            Identity,
        },
        frame_system::pallet_prelude::BlockNumberFor,
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

    impl From<StoredDispatch> for StoredDelayedDispatch {
        fn from(value: StoredDispatch) -> Self {
            StoredDelayedDispatch::new(value.kind, value.message)
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
        (StoredDispatch, Interval<BlockNumberFor<T>>),
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
        (StoredDispatch, Interval<BlockNumberFor<T>>),
    >;
}

#[cfg(test)]
#[cfg(feature = "try-runtime")]
mod tests {
    use crate::{
        migrations::{v2, MigrateToV3},
        mock::*,
        DispatchStash, Dispatches, Waitlist,
    };
    use common::storage::{Interval, LinkedNode};
    use frame_support::{pallet_prelude::StorageVersion, traits::OnRuntimeUpgrade};
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{
            DispatchKind, MessageDetails, Payload, ReplyDetails, SignalDetails, StoredMessage,
        },
    };
    use gear_core_errors::{ReplyCode, SignalCode, SuccessReplyReason};
    use rand::random;
    use sp_runtime::traits::Zero;

    fn random_payload() -> Payload {
        Payload::try_from(up_to(8 * 1024, random::<u8>).collect::<Vec<_>>())
            .expect("Len is always smaller than max capacity")
    }

    fn up_to<T>(limit: usize, f: fn() -> T) -> impl Iterator<Item = T> {
        std::iter::from_fn(move || Some(f())).take(random::<usize>() % limit)
    }

    fn random_dispatch(no_context: bool) -> v2::StoredDispatch {
        let kind = match random::<u8>() % 4 {
            0 => DispatchKind::Init,
            1 => DispatchKind::Handle,
            2 => DispatchKind::Reply,
            3 => DispatchKind::Signal,
            _ => unreachable!(),
        };
        let details = if random() {
            if random() {
                Some(MessageDetails::Reply(ReplyDetails::new(
                    MessageId::from(random::<u64>()),
                    ReplyCode::Success(SuccessReplyReason::Auto),
                )))
            } else {
                Some(MessageDetails::Signal(SignalDetails::new(
                    MessageId::from(random::<u64>()),
                    SignalCode::RemovedFromWaitlist,
                )))
            }
        } else {
            None
        };
        let context = if no_context || random() {
            None
        } else {
            let outgoing = up_to(32, || {
                (
                    random(),
                    if random() {
                        Some(random_payload())
                    } else {
                        None
                    },
                )
            })
            .collect();
            let initialized = up_to(32, || ProgramId::from(random::<u64>())).collect();
            let awaken = up_to(32, || MessageId::from(random::<u64>())).collect();
            Some(v2::ContextStore {
                outgoing,
                reply: if random() {
                    Some(random_payload())
                } else {
                    None
                },
                initialized,
                awaken,
                reply_sent: random(),
                reservation_nonce: Default::default(),
                system_reservation: if random() { Some(random()) } else { None },
            })
        };
        v2::StoredDispatch {
            kind,
            message: StoredMessage::new(
                MessageId::from(random::<u64>()),
                ProgramId::from(random::<u64>()),
                ProgramId::from(random::<u64>()),
                random_payload(),
                random(),
                details,
            ),
            context,
        }
    }

    #[test]
    fn migration_to_v3_works() {
        new_test_ext().execute_with(|| {
            StorageVersion::new(2).put::<GearMessenger>();

            let waitlist = up_to(32, || {
                (
                    ProgramId::from(random::<u64>()),
                    MessageId::from(random::<u64>()),
                    random_dispatch(false),
                    Interval {
                        start: random(),
                        finish: random(),
                    },
                )
            })
            .collect::<Vec<_>>();

            for (pid, mid, dispatch, interval) in waitlist.iter() {
                v2::Waitlist::<Test>::insert(pid, mid, (dispatch, interval));
            }

            let dispatches = up_to(32, || {
                (
                    MessageId::from(random::<u64>()),
                    random_dispatch(false),
                    if random() {
                        Some(MessageId::from(random::<u64>()))
                    } else {
                        None
                    },
                )
            })
            .collect::<Vec<_>>();

            for (mid, dispatch, next_mid) in dispatches.clone() {
                v2::Dispatches::<Test>::insert(
                    mid,
                    LinkedNode {
                        next: next_mid,
                        value: dispatch,
                    },
                );
            }

            let dispatch_stash = up_to(32, || {
                (
                    MessageId::from(random::<u64>()),
                    random_dispatch(true),
                    Interval {
                        start: random(),
                        finish: random(),
                    },
                )
            })
            .collect::<Vec<_>>();

            for (msg_id, dispatch, interval) in dispatch_stash.clone() {
                v2::DispatchStash::<Test>::insert(msg_id, (dispatch.clone(), interval.clone()));
            }

            let state = MigrateToV3::<Test>::pre_upgrade().unwrap();
            let weight = MigrateToV3::<Test>::on_runtime_upgrade();
            assert!(!weight.is_zero());
            MigrateToV3::<Test>::post_upgrade(state).unwrap();

            assert_eq!(StorageVersion::get::<GearMessenger>(), 3);

            for dispatch in waitlist {
                assert_eq!(
                    Waitlist::<Test>::get(dispatch.0, dispatch.1)
                        .expect("Waitlist failed to migrate"),
                    (dispatch.2.into(), dispatch.3)
                );
            }

            for dispatch in dispatches {
                let node =
                    Dispatches::<Test>::get(dispatch.0).expect("Dispatches failed to migrate");
                assert_eq!(node.value, dispatch.1.into());
                assert_eq!(node.next, dispatch.2);
            }

            for dispatch in dispatch_stash {
                assert_eq!(
                    DispatchStash::<Test>::get(dispatch.0)
                        .expect("DispatchStash failed to migrate"),
                    (dispatch.1.into(), dispatch.2)
                );
            }
        });
    }
}
