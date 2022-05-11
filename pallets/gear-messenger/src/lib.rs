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
    use common::storage::{
        Callback as GearCallback, DequeError as GearDequeError, Messenger as GearMessenger,
        NextKey, Node, StorageCounter as GearStorageCounter, StorageDeque as GearStorageDeque,
        StorageFlag as GearStorageFlag, StorageMap as GearStorageMap,
        StorageValue as GearStorageValue,
    };
    use frame_support::{
        pallet_prelude::*,
        traits::{ConstBool, StorageVersion},
    };
    use frame_system::pallet_prelude::*;
    use gear_core::message::StoredDispatch;
    use scale_info::TypeInfo;
    use sp_std::{convert::TryInto, marker::PhantomData, prelude::*};

    /// The current storage version.
    const MESSENGER_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Because this pallet emits events, it depends on the runtime's definition of an event.
        type Event: From<Event<Self>>
            + IsType<<Self as frame_system::Config>::Event>
            + TryInto<Event<Self>>;
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(MESSENGER_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[pallet::event]
    pub enum Event<T> {}

    #[pallet::error]
    pub enum Error<T> {
        MessageDequeFlagsCorrupted,
        MessageDequeElementsCorrupted,
    }

    impl<T> From<GearDequeError> for Error<T> {
        fn from(err: GearDequeError) -> Self {
            use GearDequeError::*;

            match err {
                ElementNotFound | DuplicateElementKey | HeadNotFoundInElements => {
                    Self::MessageDequeElementsCorrupted
                }
                _ => Self::MessageDequeFlagsCorrupted,
            }
        }
    }

    /// Numeric type defining the maximum amount of messages in queue.
    pub type LengthType = u128;

    /// Key for having access for messages in storage.
    ///
    /// Used instead of `MessageId` for space saving.
    #[derive(TypeInfo, Encode, Decode, Debug, Clone)]
    pub struct MessageKey(LengthType);

    impl<V> NextKey<V> for MessageKey {
        fn first(_target: &V) -> Self {
            Self(Default::default())
        }

        fn next(&self, _target: &V) -> Self {
            if self.0 == LengthType::MAX {
                Self(0)
            } else {
                Self(self.0 + 1)
            }
        }
    }

    #[pallet::storage]
    type Head<T> = StorageValue<_, MessageKey>;

    /// Accessor type for head of the deque.
    pub struct HeadImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageValue for HeadImpl<T> {
        type Value = MessageKey;

        fn get() -> Option<Self::Value> {
            Head::<T>::get()
        }

        fn mutate<R>(f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R {
            Head::<T>::mutate(f)
        }

        fn remove() -> Option<Self::Value> {
            Head::<T>::take()
        }

        fn set(value: Self::Value) -> Option<Self::Value> {
            Head::<T>::mutate(|v| v.replace(value))
        }
    }

    #[pallet::storage]
    type Tail<T> = StorageValue<_, MessageKey>;

    /// Accessor type for tail of the deque.
    pub struct TailImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageValue for TailImpl<T> {
        type Value = MessageKey;

        fn get() -> Option<Self::Value> {
            Tail::<T>::get()
        }

        fn mutate<R>(f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R {
            Tail::<T>::mutate(f)
        }

        fn remove() -> Option<Self::Value> {
            Tail::<T>::take()
        }

        fn set(value: Self::Value) -> Option<Self::Value> {
            Tail::<T>::mutate(|v| v.replace(value))
        }
    }

    #[pallet::storage]
    type Length<T> = StorageValue<_, LengthType, ValueQuery>;

    /// Accessor type for length of the deque.
    pub struct LengthImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageCounter for LengthImpl<T> {
        type Value = LengthType;

        fn get() -> Self::Value {
            Length::<T>::get()
        }

        fn increase() {
            Length::<T>::mutate(|v| *v = v.saturating_add(1))
        }

        fn decrease() {
            Length::<T>::mutate(|v| *v = v.saturating_sub(1))
        }

        fn clear() {
            let _prev = Length::<T>::take();
        }
    }

    #[pallet::storage]
    type Dispatches<T> = StorageMap<_, Identity, MessageKey, Node<MessageKey, StoredDispatch>>;

    /// Accessor type for elements of the deque.
    pub struct DispatchImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageMap for DispatchImpl<T> {
        type Key = MessageKey;
        type Value = Node<MessageKey, StoredDispatch>;

        fn contains(key: &Self::Key) -> bool {
            Dispatches::<T>::contains_key(key)
        }

        fn get(key: &Self::Key) -> Option<Self::Value> {
            Dispatches::<T>::get(key)
        }

        fn mutate<R>(key: Self::Key, f: impl FnOnce(&mut Option<Self::Value>) -> R) -> R {
            Dispatches::<T>::mutate(key, f)
        }

        fn remove(key: Self::Key) -> Option<Self::Value> {
            Dispatches::<T>::take(key)
        }

        fn set(key: Self::Key, value: Self::Value) -> Option<Self::Value> {
            Dispatches::<T>::mutate(key, |v| v.replace(value))
        }
    }

    /// Callback function accessor for `pop_front` action.
    pub struct OnPopFrontCallback<T>(PhantomData<T>);

    impl<T: Config> GearCallback<<DequeImpl<T> as GearStorageDeque>::Value> for OnPopFrontCallback<T> {
        fn call(_arg: &<DequeImpl<T> as GearStorageDeque>::Value) {
            <Pallet<T> as GearMessenger>::Dequeued::increase();
        }
    }

    /// Callback function accessor for `push_front` action.
    pub struct OnPushFrontCallback<T>(PhantomData<T>);

    impl<T: Config> GearCallback<<DequeImpl<T> as GearStorageDeque>::Value> for OnPushFrontCallback<T> {
        fn call(_arg: &<DequeImpl<T> as GearStorageDeque>::Value) {
            <Pallet<T> as GearMessenger>::Dequeued::decrease();
            <Pallet<T> as GearMessenger>::QueueProcessing::deny();
        }
    }

    /// Deque type itself. Contains all methods by aggregating all accessors.
    ///
    /// Never call `push_front` for priority queueing.
    /// This method should be used only for requeueing of the element which already was in the queue before.
    /// It triggers callback of decrementing `Dequeued` and denying `QueueProcessing`.
    pub struct DequeImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageDeque for DequeImpl<T> {
        type Key = MessageKey;
        type Value = StoredDispatch;

        type Error = Error<T>;

        type HeadKey = HeadImpl<T>;
        type TailKey = TailImpl<T>;
        type Elements = DispatchImpl<T>;
        type Length = LengthImpl<T>;

        type OnPopFront = OnPopFrontCallback<T>;
        type OnPushFront = OnPushFrontCallback<T>;
        type OnPushBack = ();
    }

    /// Numeric type defining the maximum amount of messages can be sent
    /// from outside (extrinsics) or processed in single block.
    pub type MessengerCapacity = u32;

    #[pallet::storage]
    type Sent<T> = StorageValue<_, MessengerCapacity, ValueQuery>;

    /// Accessor type for amount for messages sent from outside during the block.
    pub struct SentImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageCounter for SentImpl<T> {
        type Value = MessengerCapacity;

        fn get() -> Self::Value {
            Sent::<T>::get()
        }

        fn increase() {
            Sent::<T>::mutate(|v| *v = v.saturating_add(1))
        }

        fn decrease() {
            Sent::<T>::mutate(|v| *v = v.saturating_sub(1))
        }

        fn clear() {
            let _prev = Sent::<T>::take();
        }
    }

    #[pallet::storage]
    type Dequeued<T> = StorageValue<_, MessengerCapacity, ValueQuery>;

    /// Accessor type for amount for messages dequeued and appropriately
    /// processed (executed, skipped, etc.) during the block.
    pub struct DequeuedImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageCounter for DequeuedImpl<T> {
        type Value = MessengerCapacity;

        fn get() -> Self::Value {
            Dequeued::<T>::get()
        }

        fn increase() {
            Dequeued::<T>::mutate(|v| *v = v.saturating_add(1))
        }

        fn decrease() {
            Dequeued::<T>::mutate(|v| *v = v.saturating_sub(1))
        }

        fn clear() {
            let _prev = Dequeued::<T>::take();
        }
    }

    #[pallet::storage]
    type QueueProcessing<T> = StorageValue<_, bool, ValueQuery, ConstBool<true>>;

    /// Accessor type for flag showing may the queue processing be continued during the block.
    pub struct QueueProcessingImpl<T>(PhantomData<T>);

    impl<T: Config> GearStorageFlag for QueueProcessingImpl<T> {
        fn allow() {
            QueueProcessing::<T>::put(true);
        }

        fn deny() {
            QueueProcessing::<T>::put(false);
        }

        fn allowed() -> bool {
            QueueProcessing::<T>::get()
        }
    }

    impl<T: Config> GearMessenger for Pallet<T> {
        type Sent = SentImpl<T>;
        type Dequeued = DequeuedImpl<T>;
        type QueueProcessing = QueueProcessingImpl<T>;
        type Queue = DequeImpl<T>;
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        /// Initialization
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            let mut weight = 0;

            // Removes value from storage. Single DB write.
            <Self as GearMessenger>::Sent::clear();
            weight += T::DbWeight::get().writes(1);

            // Removes value from storage. Single DB write.
            <Self as GearMessenger>::Dequeued::clear();
            weight += T::DbWeight::get().writes(1);

            // Puts value in storage. Single DB write.
            <Self as GearMessenger>::QueueProcessing::allow();
            weight += T::DbWeight::get().writes(1);

            weight
        }
    }
}
