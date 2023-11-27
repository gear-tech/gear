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

pub struct MigrateToV4<T: Config>(PhantomData<T>);

impl<T: Config> MigrateToV4<T> {
    fn migrate_context_store(ctx: v3::ContextStore) -> ContextStore {
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

impl<T: Config> OnRuntimeUpgrade for MigrateToV4<T> {
    #[cfg(feature = "try-runtime")]
    fn on_runtime_upgrade() -> Weight {

        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        log::info!(
            "🚚 Running migration with current storage version {current:?} / onchain {onchain:?}"
        );

        // 1 read for on chain storage version
        let mut weight = T::DbWeight::get().reads(1);

        if current == 4 && onchain == 3 {
            Waitlist::<T>::translate(|_, _, store: (v3::StoredDispatch, Interval<T::BlockNumber>)| {
                Some((StoredDispatch {
                    kind: store.0.kind,
                    message: store.0.message,
                    context: store.0.context.map(MigrateToV4::migrate_context_store),
                }, store.1))
            });

            Dispatches::<T>::translate(|a, store: LinkedNode<MessageId, v3::StoredDispatch>| {
                Some(LinkedNode {
                    next: store.next,
                    value: Self::migrate_context_store(store.value),
                })
            });
        }

        Weight::zero()
    }
}

mod v3 {
    use std::collections::{BTreeMap, BTreeSet};
    use frame_support::{codec::{Decode, Encode}, Identity, scale_info::{TypeInfo}};
    use frame_support::pallet_prelude::{CountedStorageMap, StorageDoubleMap};
    use common::storage::{Interval, LinkedNode};
    use gear_core::ids::{MessageId, ProgramId};
    use gear_core::message::{DispatchKind, Payload, StoredMessage};
    use gear_core::reservation::ReservationNonce;
    use crate::Config;

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

    #[cfg(feature = "try-runtime")]
    pub type Dispatches<T> = CountedStorageMap<_, Identity, MessageId, LinkedNode<MessageId, StoredDispatch>>;

    #[cfg(feature = "try-runtime")]
    pub type Waitlist<T: Config> = StorageDoubleMap<
        _,
        Identity,
        ProgramId,
        Identity,
        MessageId,
        (StoredDispatch, Interval<T::BlockNumber>),
    >;

}