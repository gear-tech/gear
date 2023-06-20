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

use crate::{Config, Pallet};
use frame_support::{
    traits::{Get, GetStorageVersion, OnRuntimeUpgrade},
    weights::Weight,
};
use sp_std::marker::PhantomData;

#[cfg(feature = "try-runtime")]
use {
    frame_support::{
        codec::{Decode, Encode},
        traits::StorageVersion,
    },
    sp_std::vec::Vec,
};

pub struct MigrateToV2<T: Config>(PhantomData<T>);

impl<T: Config> OnRuntimeUpgrade for MigrateToV2<T> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, &'static str> {
        // Currently running on chain storage version of the pallets storage.
        let version = <Pallet<T>>::on_chain_storage_version();

        Ok(version.encode())
    }

    fn on_runtime_upgrade() -> Weight {
        // Versions query.
        let current = Pallet::<T>::current_storage_version();
        let onchain = Pallet::<T>::on_chain_storage_version();

        // Debug information.
        log::info!(
            "🚚 Running migration with current storage version {:?} / onchain {:?}",
            current,
            onchain
        );

        // Total weight of migration.
        //
        // Starts from single read: query of current storages version above.
        let mut weight = T::DbWeight::get().reads(1);

        // Function of increasing weight per each translated value in storage.
        //
        // Firstly we read each value, then process it inside
        // `translate` closure, writing new value afterward.
        let mut add_translated = || {
            weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
        };

        // Check defining should we execute migrations or not.
        if current == 2 && onchain == 1 {
            // Translation of `Dispatches` storage.
            crate::Dispatches::<T>::translate_values(|value| {
                add_translated();
                Some(transition::dispatches(value))
            });

            // Translation of `Mailbox` storage.
            crate::Mailbox::<T>::translate_values(|value| {
                add_translated();
                Some(transition::mailbox::<T>(value))
            });

            // Translation of `Waitlist` storage.
            crate::Waitlist::<T>::translate_values(|value| {
                add_translated();
                Some(transition::waitlist::<T>(value))
            });

            // Translation of `DispatchStash` storage.
            crate::DispatchStash::<T>::translate_values(|value| {
                add_translated();
                Some(transition::dispatch_stash::<T>(value))
            });

            // Adding weight for write of newly updated storage version.
            weight = weight.saturating_add(T::DbWeight::get().writes(1));
            current.put::<Pallet<T>>();

            // Success debug information.
            log::info!("Successfully migrated storage from v1 to v2");
        } else {
            // Skipped debug information.
            log::info!("❌ Migration did not execute. This probably should be removed");
        }

        // Total weight of migration.
        weight
    }

    #[cfg(feature = "try-runtime")]
    fn post_upgrade(state: Vec<u8>) -> Result<(), &'static str> {
        // Query of pre-runtime upgrade version of pallets storage.
        let old_version: StorageVersion =
            Decode::decode(&mut state.as_ref()).map_err(|_| "Cannot decode version")?;

        // Query of newly updated on chain version of pallets storage.
        let onchain_version = Pallet::<T>::on_chain_storage_version();

        // Assertion that version changed.
        assert_ne!(
            onchain_version, old_version,
            "must have upgraded from version 1 to 2."
        );

        // Debug information.
        log::info!("Storage successfully migrated.");
        Ok(())
    }
}

mod v1 {
    use frame_support::{
        codec::{self, Decode, Encode},
        scale_info::{self, TypeInfo},
        storage_alias, Identity,
    };

    // Pay attention that these types were taken from
    // actual codebase due to changes absence.
    use common::storage::{Interval, LinkedNode};
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{ContextStore, DispatchKind, Payload},
    };

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub enum MessageDetails {
        Reply(ReplyDetails),
        Signal(SignalDetails),
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct ReplyDetails {
        pub reply_to: MessageId,
        pub status_code: i32,
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct SignalDetails {
        pub from: MessageId,
        pub status_code: i32,
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct StoredMessage {
        pub id: MessageId,
        pub source: ProgramId,
        pub destination: ProgramId,
        pub payload: Payload,
        #[codec(compact)]
        pub value: u128,
        pub details: Option<MessageDetails>,
    }

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
    #[codec(crate = codec)]
    #[scale_info(crate = scale_info)]
    pub struct StoredDispatch {
        pub kind: DispatchKind,
        pub message: StoredMessage,
        pub context: Option<ContextStore>,
    }

    #[storage_alias]
    pub type Dispatches<T: crate::Config> = CountedStorageMap<
        crate::Pallet<T>,
        Identity,
        MessageId,
        LinkedNode<MessageId, StoredDispatch>,
    >;

    #[storage_alias]
    pub type Mailbox<T: crate::Config> = StorageDoubleMap<
        crate::Pallet<T>,
        Identity,
        <T as frame_system::Config>::AccountId,
        Identity,
        MessageId,
        (
            StoredMessage,
            Interval<<T as frame_system::Config>::BlockNumber>,
        ),
    >;

    #[storage_alias]
    pub type Waitlist<T: crate::Config> = StorageDoubleMap<
        crate::Pallet<T>,
        Identity,
        ProgramId,
        Identity,
        MessageId,
        (
            StoredDispatch,
            Interval<<T as frame_system::Config>::BlockNumber>,
        ),
    >;

    #[storage_alias]
    pub type DispatchStash<T: crate::Config> = StorageMap<
        crate::Pallet<T>,
        Identity,
        MessageId,
        (
            StoredDispatch,
            Interval<<T as frame_system::Config>::BlockNumber>,
        ),
    >;
}

mod transition {
    use crate::Config;

    // Old types.
    use super::v1;

    // Actual and unchanged types.
    use common::storage::{Interval, LinkedNode};
    use gear_core::{
        ids::MessageId,
        message::{
            MessageDetails, ReplyDetails, SignalDetails, StoredDispatch, StoredMessage,
            UserStoredMessage,
        },
    };
    use gear_core_errors::{
        ErrorReason, ReplyCode, SignalCode, SimpleExecutionError, SuccessReason,
    };

    fn reply_details(old_details: v1::ReplyDetails) -> ReplyDetails {
        let to = old_details.reply_to;

        let code = if old_details.status_code == 0 {
            ReplyCode::Success(SuccessReason::Unsupported)
        } else {
            ReplyCode::Error(ErrorReason::Unsupported)
        };

        ReplyDetails::new(to, code)
    }

    fn signal_details(old_details: v1::SignalDetails) -> SignalDetails {
        let to = old_details.from;

        let code = SignalCode::Execution(SimpleExecutionError::Unsupported);

        SignalDetails::new(to, code)
    }

    fn message_details(old_details: v1::MessageDetails) -> MessageDetails {
        match old_details {
            v1::MessageDetails::Reply(old_reply_details) => {
                MessageDetails::Reply(reply_details(old_reply_details))
            }
            v1::MessageDetails::Signal(old_signal_details) => {
                MessageDetails::Signal(signal_details(old_signal_details))
            }
        }
    }

    fn stored_message(old_message: v1::StoredMessage) -> StoredMessage {
        StoredMessage::new(
            old_message.id,
            old_message.source,
            old_message.destination,
            old_message.payload,
            old_message.value,
            old_message.details.map(message_details),
        )
    }

    fn stored_dispatch(old_dispatch: v1::StoredDispatch) -> StoredDispatch {
        StoredDispatch::new(
            old_dispatch.kind,
            stored_message(old_dispatch.message),
            old_dispatch.context,
        )
    }

    fn user_stored_message(old_message: v1::StoredMessage) -> UserStoredMessage {
        let stored_message = stored_message(old_message);

        stored_message
            .try_into()
            .unwrap_or_else(|_| unreachable!("Signal messages must never be sent to user!"))
    }

    pub fn dispatches(
        old_value: LinkedNode<MessageId, v1::StoredDispatch>,
    ) -> LinkedNode<MessageId, StoredDispatch> {
        LinkedNode {
            next: old_value.next,
            value: stored_dispatch(old_value.value),
        }
    }

    pub fn mailbox<T: Config>(
        old_value: (v1::StoredMessage, Interval<T::BlockNumber>),
    ) -> (UserStoredMessage, Interval<T::BlockNumber>) {
        (user_stored_message(old_value.0), old_value.1)
    }

    pub fn waitlist<T: Config>(
        old_value: (v1::StoredDispatch, Interval<T::BlockNumber>),
    ) -> (StoredDispatch, Interval<T::BlockNumber>) {
        (stored_dispatch(old_value.0), old_value.1)
    }

    pub fn dispatch_stash<T: Config>(
        old_value: (v1::StoredDispatch, Interval<T::BlockNumber>),
    ) -> (StoredDispatch, Interval<T::BlockNumber>) {
        (stored_dispatch(old_value.0), old_value.1)
    }
}

#[cfg(all(test, feature = "try-runtime"))]
mod tests {
    use super::v1;
    use crate::mock;
    use common::{
        storage::{Interval, LinkedNode},
        Origin as _,
    };
    use frame_support::{
        traits::{Get, GetStorageVersion, OnRuntimeUpgrade, StorageVersion},
        weights::RuntimeDbWeight,
    };
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::DispatchKind,
    };
    use primitive_types::H256;

    fn generate_dispatch() -> v1::StoredDispatch {
        let id = H256::random();
        let byte: u8 = id.as_ref()[0];

        v1::StoredDispatch {
            kind: match byte % 4 {
                0 => DispatchKind::Init,
                1 => DispatchKind::Handle,
                2 => DispatchKind::Reply,
                _ => DispatchKind::Signal,
            },
            message: v1::StoredMessage {
                id: MessageId::from_origin(id),
                source: ProgramId::from_origin(H256::random()),
                destination: ProgramId::from_origin(H256::random()),
                payload: H256::random()
                    .as_ref()
                    .to_vec()
                    .try_into()
                    .expect("Infallible"),
                value: byte as u128 * 12_345,
                details: match byte % 4 {
                    0 | 1 => None,
                    2 => Some(v1::MessageDetails::Reply(v1::ReplyDetails {
                        reply_to: MessageId::from_origin(H256::random()),
                        status_code: byte as i32 % 2,
                    })),
                    _ => Some(v1::MessageDetails::Signal(v1::SignalDetails {
                        from: MessageId::from_origin(H256::random()),
                        status_code: byte as i32,
                    })),
                },
            },
            context: (byte % 2 == 0).then(Default::default),
        }
    }

    #[test]
    fn migrate_v1_to_v2() {
        let _ = env_logger::try_init();

        mock::new_test_ext().execute_with(|| {
            // Setting previous storage version.
            StorageVersion::new(1).put::<mock::GearMessenger>();

            let interval = Interval::<<mock::Test as frame_system::Config>::BlockNumber> {
                start: 1,
                finish: 101,
            };

            // `Dispatches` insertion.
            let dispatches = (0..10).map(|_| generate_dispatch()).collect::<Vec<_>>();

            for dispatch in dispatches.clone() {
                v1::Dispatches::<mock::Test>::insert(
                    dispatch.message.id,
                    LinkedNode {
                        next: None,
                        value: dispatch,
                    },
                );
            }

            // `Waitlist` insertion.
            let waitlisted = (0..10).map(|_| generate_dispatch()).collect::<Vec<_>>();

            for dispatch in waitlisted.clone() {
                v1::Waitlist::<mock::Test>::insert(
                    dispatch.message.destination,
                    dispatch.message.id,
                    (dispatch, interval.clone()),
                );
            }

            // `DispatchStash` insertion.
            let stashed = (0..10).map(|_| generate_dispatch()).collect::<Vec<_>>();

            for dispatch in stashed.clone() {
                v1::DispatchStash::<mock::Test>::insert(
                    dispatch.message.id,
                    (dispatch, interval.clone()),
                );
            }

            // `Mailbox` insertion.
            let mailboxed = (0..40)
                .filter_map(|_| {
                    let dispatch = generate_dispatch();
                    (dispatch.kind == DispatchKind::Handle).then_some(dispatch.message)
                })
                .collect::<Vec<_>>();

            for message in mailboxed.clone() {
                v1::Mailbox::<mock::Test>::insert(
                    <mock::Test as frame_system::Config>::AccountId::from_origin(
                        message.id.into_origin(),
                    ),
                    message.id,
                    (message, interval.clone()),
                );
            }

            // Total count of messages.
            let transmuted = dispatches.len() + waitlisted.len() + stashed.len() + mailboxed.len();
            // Total amount of read writes equals total count of messages and version read and write.
            let total = (transmuted + 1)
                .try_into()
                .expect("Read-writes count overflow");
            let expected_weight =
                <<mock::Test as frame_system::Config>::DbWeight as Get<RuntimeDbWeight>>::get()
                    .reads_writes(total, total);

            let state =
                super::MigrateToV2::<mock::Test>::pre_upgrade().expect("pre_upgrade failed");
            let weight = super::MigrateToV2::<mock::Test>::on_runtime_upgrade();

            assert_eq!(weight.ref_time(), expected_weight.ref_time());

            super::MigrateToV2::<mock::Test>::post_upgrade(state).unwrap();

            // Asserting amount of messages.
            assert_eq!(
                crate::Dispatches::<mock::Test>::iter().count(),
                dispatches.len()
            );
            assert_eq!(
                crate::Waitlist::<mock::Test>::iter().count(),
                waitlisted.len()
            );
            assert_eq!(
                crate::DispatchStash::<mock::Test>::iter().count(),
                stashed.len()
            );
            assert_eq!(
                crate::Mailbox::<mock::Test>::iter().count(),
                mailboxed.len()
            );
            // Asserting version set.
            assert_eq!(
                mock::GearMessenger::on_chain_storage_version(),
                crate::MESSENGER_STORAGE_VERSION
            );
        });
    }
}
