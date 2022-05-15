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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod migration;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use super::*;
    use common::{storage::*, Origin};
    use frame_support::{pallet_prelude::*, traits::StorageVersion};
    use frame_system::pallet_prelude::*;
    use gear_core::{
        ids::MessageId,
        message::{StoredDispatch, StoredMessage},
    };
    use sp_std::{convert::TryInto, marker::PhantomData, vec::Vec};

    /// The current storage version.
    const MESSENGER_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {}

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(MESSENGER_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::error]
    pub enum Error<T> {
        QueueDuplicateKey,
        QueueElementNotFound,
        QueueHeadShouldBe,
        QueueHeadShouldNotBe,
        QueueTailHasNextKey,
        QueueTailParentNotFound,
        QueueTailShouldBe,
        QueueTailShouldNotBe,
        QueueRemoveAllFailed,
        MailboxDuplicateKey,
        MailboxElementNotFound,
    }

    impl<T: crate::Config> LinkedListError for Error<T> {
        fn duplicate_key() -> Self {
            Self::QueueDuplicateKey
        }

        fn element_not_found() -> Self {
            Self::QueueElementNotFound
        }

        fn head_should_be() -> Self {
            Self::QueueHeadShouldBe
        }

        fn head_should_not_be() -> Self {
            Self::QueueHeadShouldNotBe
        }

        fn tail_has_next_key() -> Self {
            Self::QueueTailHasNextKey
        }

        fn tail_parent_not_found() -> Self {
            Self::QueueTailParentNotFound
        }

        fn tail_should_be() -> Self {
            Self::QueueTailShouldBe
        }

        fn tail_should_not_be() -> Self {
            Self::QueueTailShouldNotBe
        }
    }

    impl<T: crate::Config> MailboxError for Error<T> {
        fn duplicate_key() -> Self {
            Self::MailboxDuplicateKey
        }

        fn element_not_found() -> Self {
            Self::MailboxElementNotFound
        }
    }

    /// Numeric type defining the maximum amount of messages can be sent
    /// from outside (extrinsics) or processed in single block.
    pub type Capacity = u32;

    #[pallet::storage]
    type Head<T> = StorageValue<_, MessageId>;

    common::wrap_storage_value!(storage: Head, name: HeadWrap, value: MessageId);

    #[pallet::storage]
    type Tail<T> = StorageValue<_, MessageId>;

    common::wrap_storage_value!(storage: Tail, name: TailWrap, value: MessageId);

    #[pallet::storage]
    type Dispatches<T> =
        CountedStorageMap<_, Identity, MessageId, LinkedNode<MessageId, StoredDispatch>>;

    common::wrap_counted_storage_map!(
        storage: Dispatches,
        name: DispatchesWrap,
        key: MessageId,
        value: LinkedNode<MessageId, StoredDispatch>,
        length: Capacity
    );

    #[pallet::storage]
    type Sent<T> = StorageValue<_, Capacity>;

    common::wrap_storage_value!(storage: Sent, name: SentWrap, value: Capacity);

    #[pallet::storage]
    type Dequeued<T> = StorageValue<_, Capacity>;

    common::wrap_storage_value!(storage: Dequeued, name: DequeuedWrap, value: Capacity);

    #[pallet::storage]
    type QueueProcessing<T> = StorageValue<_, bool>;

    common::wrap_storage_value!(
        storage: QueueProcessing,
        name: QueueProcessingWrap,
        value: bool
    );

    #[pallet::storage]
    type Mailbox<T: Config> =
        StorageDoubleMap<_, Identity, T::AccountId, Identity, MessageId, StoredMessage>;

    common::wrap_storage_double_map!(
        storage: Mailbox,
        name: MailboxWrap,
        key1: T::AccountId,
        key2: MessageId,
        value: StoredMessage
    );

    /// Callback function accessor for `pop_front` action.
    pub struct OnPopFront<V, T>(PhantomData<(V, T)>);

    impl<V, T: Messenger> Callback<V> for OnPopFront<V, T> {
        fn call(_arg: &V) {
            T::Dequeued::increase();
        }
    }

    /// Callback function accessor for `push_front` action.
    pub struct OnPushFront<V, T>(PhantomData<(V, T)>);

    impl<V, T: Messenger> Callback<V> for OnPushFront<V, T> {
        fn call(_arg: &V) {
            T::Dequeued::decrease();
            T::QueueProcessing::deny();
        }
    }

    pub struct QueueCallbacks<V, T>(PhantomData<(V, T)>);

    impl<V, T: Messenger> LinkedListCallbacks for QueueCallbacks<V, T> {
        type Value = V;

        type OnPopBack = ();
        type OnPopFront = OnPopFront<V, T>;
        type OnPushBack = ();
        type OnPushFront = OnPushFront<V, T>;
        type OnRemoveAll = ();
    }

    pub struct MailBoxCallbacks<V, T>(PhantomData<(V, T)>);

    impl<V, T: Messenger> MailboxCallbacks for MailBoxCallbacks<V, T> {
        type Value = V;

        type OnInsert = ();
        type OnRemove = ();
    }

    /// Message processing centralized behaviour.
    impl<T: crate::Config> Messenger for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type Capacity = Capacity;
        type Error = Error<T>;

        type MailboxFirstKey = T::AccountId;
        type MailboxSecondKey = MessageId;
        type MailboxedMessage = StoredMessage;
        type QueuedDispatch = StoredDispatch;

        /// Amount of messages sent from outside.
        type Sent = CounterImpl<Self::Capacity, SentWrap<T>>;

        /// Amount of messages dequeued.
        type Dequeued = CounterImpl<Self::Capacity, DequeuedWrap<T>>;

        /// Allowance of queue processing.
        type QueueProcessing = TogglerImpl<QueueProcessingWrap<T>>;

        /// Message queue store.
        type Queue = QueueImpl<
            LinkedListImpl<
                MessageId,
                Self::QueuedDispatch,
                Self::Error,
                HeadWrap<T>,
                TailWrap<T>,
                DispatchesWrap<T>,
                QueueCallbacks<Self::QueuedDispatch, Self>,
            >,
            QueueKeyGen,
        >;

        /// Users mailbox store.
        type Mailbox = MailboxImpl<
            MailboxWrap<T>,
            Self::Error,
            MailBoxCallbacks<Self::MailboxedMessage, Self>,
            MailboxKeyGen<T::AccountId>,
        >;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            let mut weight = 0;

            // Removes value from storage. Single DB write.
            <Self as Messenger>::Sent::reset();
            weight += T::DbWeight::get().writes(1);

            // Removes value from storage. Single DB write.
            <Self as Messenger>::Dequeued::reset();
            weight += T::DbWeight::get().writes(1);

            // Puts value in storage. Single DB write.
            <Self as Messenger>::QueueProcessing::allow();
            weight += T::DbWeight::get().writes(1);

            weight
        }
    }
}
