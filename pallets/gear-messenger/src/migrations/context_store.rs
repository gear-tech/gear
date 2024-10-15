use core::marker::PhantomData;

use crate::{Config, Pallet, Waitlist};
use common::{
    storage::{Interval, LinkedNode, Queue},
    MessageId,
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

const MIGRATE_FROM_VERSION: u16 = 10;
const MIGRATE_TO_VERSION: u16 = 11;
const ALLOWED_CURRENT_STORAGE_VERSION: u16 = 10;

fn translate_dispatch(dispatch: v10::StoredDispatch) -> StoredDispatch {
    StoredDispatch::new(
        dispatch.kind,
        dispatch.message,
        dispatch.context.map(|ctx| {
            ContextStore::new(
                ctx.initialized,
                ctx.reservation_nonce,
                ctx.system_reservation,
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
            let current = Pallet::<T>::current_storage_version();
            if current != ALLOWED_CURRENT_STORAGE_VERSION {
                log::error!("❌ Migration is not allowed for current storage version {current:?}.");
                return weight;
            }

            let update_to = StorageVersion::new(MIGRATE_TO_VERSION);
            log::info!("🚚 Running migration from {onchain:?} to {update_to:?}, current storage version is {current:?}.");

            Dispatches::<T>::translate(|_, value: LinkedNode<MessageId, v10::StoredDispatch>| {
                counter += 1;
                Some(LinkedNode {
                    next: value.next,
                    value: translate_dispatch(value.value),
                })
            });

            Waitlist::<T>::translate(
                |_, _, (dispatch, interval): (v10::StoredDispatch, Interval<BlockNumberFor<T>>)| {
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
}

mod v10 {
    use common::{storage::LinkedNode, MessageId, ProgramId};
    use core::marker::PhantomData;
    use frame_support::{
        pallet_prelude::CountedStorageMap, storage::types::CountedStorageMapInstance,
        traits::StorageInstance, Identity,
    };
    use frame_system::Config;
    use gear_core::{
        message::{DispatchKind, Payload, StoredMessage},
        reservation::ReservationNonce,
    };

    use scale_info::{
        scale::{Decode, Encode},
        TypeInfo,
    };
    use std::collections::{BTreeMap, BTreeSet};

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
