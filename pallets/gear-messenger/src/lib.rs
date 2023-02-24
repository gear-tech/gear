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

//! # Gear Messenger Pallet
//!
//! The Gear Messenger Pallet provides functionality for handling messages.
//!
//! - [`Config`]
//! - [`Pallet`]
//!
//! ## Overview
//!
//! The Gear Messenger Pallet's main aim is to separate message storages out
//! of Gear's execution logic and provide soft functionality to manage them.
//!
//! The Gear Messenger Pallet provides functions for:
//! - Counting amount of messages sent from outside (from extrinsics)
//! within the current block.
//! - Counting amount of messages removed from queue to be processed
//! or skipped withing the current block.
//! - Managing continuation of queue processing withing the current block.
//! - Storing and managing message queue, it's pushing and popping algorithms.
//! - Storing and managing mailbox, it's insertion and removal algorithms,
//! including the value claiming with Balances Pallet as `Currency`
//! `Config`'s associated type.
//!
//! ## Interface
//!
//! The Gear Messenger Pallet implements `gear_common::storage::Messenger` trait
//! and shouldn't contain any other functionality, except this trait declares.
//!
//! ## Usage
//!
//! How to use the messaging functionality from the Gear Messenger Pallet:
//!
//! 1. Implement it's `Config` for your runtime with specified `Currency` type.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl pallet_gear_messenger::Config for Runtime {
//!     type Currency = .. ;
//! }
//!
//! // ... //
//! ```
//!
//! 2. Provide associated type for your pallet's `Config`, which implements
//! `gear_common::storage::Messenger` trait,
//! specifying associated types if needed.
//!
//! ```ignore
//! // `some_pallet/src/lib.rs`
//! // ... //
//!
//! use gear_common::storage::Messenger;
//!
//! #[pallet::config]
//! pub trait Config: frame_system::Config {
//!     // .. //
//!
//!     type Messenger: Messenger<Capacity = u32>;
//!
//!     // .. //
//! }
//! ```
//!
//! 3. Declare Gear Messenger Pallet in your `construct_runtime!` macro.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! construct_runtime!(
//!     pub enum Runtime
//!         where // ... //
//!     {
//!         // ... //
//!
//!         GearMessenger: pallet_gear_messenger,
//!
//!         // ... //
//!     }
//! );
//!
//! // ... //
//! ```
//! `GearMessenger: pallet_gear_messenger`
//!
//! 4. Set `GearMessenger` as your pallet `Config`'s `Messenger type.
//!
//! ```ignore
//! // `runtime/src/lib.rs`
//! // ... //
//!
//! impl some_pallet::Config for Runtime {
//!     // ... //
//!
//!     type Messenger = GearMessenger;
//!
//!     // ... //
//! }
//!
//! // ... //
//! ```
//!
//! 5. Work with Gear Messenger Pallet in your pallet with provided
//! associated type interface.
//!
//! ## Genesis config
//!
//! The Gear Messenger Pallet doesn't depend on the `GenesisConfig`.
//!
//! ## Assumptions
//!
//! * You should manually control storage load from queue and mailbox
//! length overflow (see Gear Payment Pallet).

#![cfg_attr(not(feature = "std"), no_std)]

// Database migration module.
pub mod migration;

// Runtime mock for running tests.
#[cfg(test)]
mod mock;

// Unit tests module.
#[cfg(test)]
mod tests;

// Public exports from pallet.
pub use pallet::*;

// Gear Messenger Pallet module.
#[frame_support::pallet]
pub mod pallet {
    pub use frame_support::weights::Weight;

    use common::{storage::*, BlockLimiter, Origin};
    use frame_support::{
        dispatch::DispatchError, pallet_prelude::*, storage::PrefixIterator, traits::StorageVersion,
    };
    use frame_system::pallet_prelude::BlockNumberFor;
    use gear_core::{
        ids::{MessageId, ProgramId},
        message::{StoredDispatch, StoredMessage},
    };
    use sp_std::{convert::TryInto, marker::PhantomData};

    /// The current storage version.
    const MESSENGER_STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    // Gear Messenger Pallet's `Config`.
    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// Block limits.
        type BlockLimiter: BlockLimiter<Balance = u64>;

        /// Custom block number tracker.
        type CurrentBlockNumber: Get<BlockNumberFor<Self>>;
    }

    // Gear Messenger Pallet itself.
    //
    // Uses without storage info to avoid direct access to pallet's
    // storage from outside.
    //
    // Uses `MESSENGER_STORAGE_VERSION` as current storage version.
    #[pallet::pallet]
    #[pallet::without_storage_info]
    #[pallet::storage_version(MESSENGER_STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    // Gear Messenger Pallet error type.
    //
    // Used as inner error type for `Messenger` implementation.
    #[pallet::error]
    pub enum Error<T> {
        /// Occurs when given key already exists in queue.
        QueueDuplicateKey,
        /// Occurs when queue's element wasn't found in storage.
        QueueElementNotFound,
        /// Occurs when queue's head should contain value,
        /// but it's empty for some reason.
        QueueHeadShouldBeSet,
        /// Occurs when queue's head should be empty,
        /// but it contains value for some reason.
        QueueHeadShouldNotBeSet,
        /// Occurs when queue's tail element contains link
        /// to the next element.
        QueueTailHasNextKey,
        /// Occurs when while searching queue's pre-tail,
        /// element wasn't found.
        QueueTailParentNotFound,
        /// Occurs when queue's tail should contain value,
        /// but it's empty for some reason.
        QueueTailShouldBeSet,
        /// Occurs when queue's tail should be empty,
        /// but it contains value for some reason.
        QueueTailShouldNotBeSet,
        /// Occurs when given value already exists in mailbox.
        MailboxDuplicateKey,
        /// Occurs when mailbox's element wasn't found in storage.
        MailboxElementNotFound,
        /// Occurs when given value already exists in waitlist.
        WaitlistDuplicateKey,
        /// Occurs when waitlist's element wasn't found in storage.
        WaitlistElementNotFound,
    }

    // Implementation of `DequeueError` for `Error<T>`
    // usage as `Queue::Error`.
    impl<T: crate::Config> DequeueError for Error<T> {
        fn duplicate_key() -> Self {
            Self::QueueDuplicateKey
        }

        fn element_not_found() -> Self {
            Self::QueueElementNotFound
        }

        fn head_should_be_set() -> Self {
            Self::QueueHeadShouldBeSet
        }

        fn head_should_not_be_set() -> Self {
            Self::QueueHeadShouldNotBeSet
        }

        fn tail_has_next_key() -> Self {
            Self::QueueTailHasNextKey
        }

        fn tail_parent_not_found() -> Self {
            Self::QueueTailParentNotFound
        }

        fn tail_should_be_set() -> Self {
            Self::QueueTailShouldBeSet
        }

        fn tail_should_not_be_set() -> Self {
            Self::QueueTailShouldNotBeSet
        }
    }

    // Implementation of `MailboxError` for `Error<T>`
    // usage as `Mailbox::Error`.
    impl<T: crate::Config> MailboxError for Error<T> {
        fn duplicate_key() -> Self {
            Self::MailboxDuplicateKey
        }

        fn element_not_found() -> Self {
            Self::MailboxElementNotFound
        }
    }

    // Implementation of `WaitlistError` for `Error<T>`
    // usage as `Waitlist::Error`.
    impl<T: crate::Config> WaitlistError for Error<T> {
        fn duplicate_key() -> Self {
            Self::WaitlistDuplicateKey
        }

        fn element_not_found() -> Self {
            Self::WaitlistElementNotFound
        }
    }

    /// Numeric type defining the maximum amount of messages can be sent
    /// from outside (from extrinsics) or processed in single block.
    pub type Capacity = u32;

    // Below goes storages and their gear's wrapper implementations.
    //
    // Note, that we declare storages private to avoid outside
    // interaction with them, but wrappers - public to be able
    // use them as generic parameters in public `Messenger`
    // implementation.

    // ----

    // Private storage for amount of messages dequeued.
    #[pallet::storage]
    type Dequeued<T> = StorageValue<_, Capacity>;

    // Public wrap of the amount of messages dequeued.
    common::wrap_storage_value!(storage: Dequeued, name: DequeuedWrap, value: Capacity);

    // ----

    // Private storage for queue's elements.
    #[pallet::storage]
    type Dispatches<T> =
        CountedStorageMap<_, Identity, MessageId, LinkedNode<MessageId, StoredDispatch>>;

    // Public wrap of the queue's elements.
    common::wrap_counted_storage_map!(
        storage: Dispatches,
        name: DispatchesWrap,
        key: MessageId,
        value: LinkedNode<MessageId, StoredDispatch>,
        length: Capacity
    );

    // ----

    // Private storage for queue's head key.
    #[pallet::storage]
    type Head<T> = StorageValue<_, MessageId>;

    // Public wrap of the queue's head key.
    common::wrap_storage_value!(storage: Head, name: HeadWrap, value: MessageId);

    // ----

    // Private storage for mailbox elements.
    #[pallet::storage]
    type Mailbox<T: Config> = StorageDoubleMap<
        _,
        Identity,
        T::AccountId,
        Identity,
        MessageId,
        (StoredMessage, Interval<T::BlockNumber>),
    >;

    // Public wrap of the mailbox elements.
    common::wrap_extended_storage_double_map!(
        storage: Mailbox,
        name: MailboxWrap,
        key1: T::AccountId,
        key2: MessageId,
        value: (StoredMessage, Interval<T::BlockNumber>),
        length: usize
    );

    // ----

    // Private storage for queue processing flag.
    #[pallet::storage]
    type QueueProcessing<T> = StorageValue<_, bool>;

    // Public wrap of the queue processing flag.
    common::wrap_storage_value!(
        storage: QueueProcessing,
        name: QueueProcessingWrap,
        value: bool
    );

    // ----

    // Private storage for amount of messages sent.
    #[pallet::storage]
    type Sent<T> = StorageValue<_, Capacity>;

    // Public wrap of the amount of messages sent.
    common::wrap_storage_value!(storage: Sent, name: SentWrap, value: Capacity);

    // ----

    // Private storage for queue's tail key.
    #[pallet::storage]
    type Tail<T> = StorageValue<_, MessageId>;

    // Public wrap of the queue's tail key.
    common::wrap_storage_value!(storage: Tail, name: TailWrap, value: MessageId);

    // ----

    // Private storage for waitlist elements.
    #[pallet::storage]
    type Waitlist<T: Config> = StorageDoubleMap<
        _,
        Identity,
        ProgramId,
        Identity,
        MessageId,
        (StoredDispatch, Interval<T::BlockNumber>),
    >;

    // Public wrap of the waitlist elements.
    common::wrap_extended_storage_double_map!(
        storage: Waitlist,
        name: WaitlistWrap,
        key1: ProgramId,
        key2: MessageId,
        value: (StoredDispatch, Interval<T::BlockNumber>),
        length: usize
    );

    // ----

    // Private storage for dispatch stash elements.
    #[pallet::storage]
    type DispatchStash<T: Config> =
        StorageMap<_, Identity, MessageId, (StoredDispatch, Interval<T::BlockNumber>)>;

    // Public wrap of the dispatch stash elements.
    common::wrap_storage_map!(
        storage: DispatchStash,
        name: DispatchStashWrap,
        key: MessageId,
        value: (StoredDispatch, Interval<T::BlockNumber>)
    );

    // ----

    // Below goes callbacks, used for queue algorithm.
    //
    // Note, that they are public like storage wrappers
    // only to be able to use as public trait's generics.

    // ----

    /// Callback function for success `pop_front` action.
    pub struct OnPopFront<T: crate::Config, V>(PhantomData<(T, V)>)
    where
        T::AccountId: Origin;

    // Callback trait implementation.
    //
    // Pop front increases amount of messages dequeued.
    impl<T: crate::Config, V> Callback<V> for OnPopFront<T, V>
    where
        T::AccountId: Origin,
    {
        fn call(_arg: &V) {
            <Pallet<T> as Messenger>::Dequeued::increase();
        }
    }

    // ---

    /// Callback function for success `push_front` action.
    pub struct OnPushFront<T: crate::Config, V>(PhantomData<(T, V)>)
    where
        T::AccountId: Origin;

    // Callback trait implementation.
    //
    // Push front means requeue in Gear Messenger Context,
    // so the dequeued amount should be decreased and
    // queue processing stopped.
    impl<T: crate::Config, V> Callback<V> for OnPushFront<T, V>
    where
        T::AccountId: Origin,
    {
        fn call(_arg: &V) {
            <Pallet<T> as Messenger>::Dequeued::decrease();
            <Pallet<T> as Messenger>::QueueProcessing::deny();
        }
    }

    // ----

    /// Store of queue action's callbacks.
    pub struct QueueCallbacks<T: crate::Config>(PhantomData<T>)
    where
        T::AccountId: Origin;

    // Callbacks store for queue trait implementation, over
    // specified (associated) type of queue value.
    impl<T: crate::Config> DequeueCallbacks for QueueCallbacks<T>
    where
        T::AccountId: Origin,
    {
        type Value = <Pallet<T> as Messenger>::QueuedDispatch;

        type OnPopBack = ();
        type OnPopFront = OnPopFront<T, Self::Value>;
        type OnPushBack = ();
        type OnPushFront = OnPushFront<T, Self::Value>;
        type OnClear = ();
    }

    // ----

    // Below goes callbacks, used for mailbox algorithm.
    //
    // Note, that they are public like storage wrappers
    // only to be able to use as public trait's generics.

    // ----

    /// Store of mailbox action's callbacks.
    pub struct MailBoxCallbacks<T: crate::Config>(PhantomData<T>)
    where
        T::AccountId: Origin;

    // Callbacks store for mailbox trait implementation, over
    // specified (associated) types of mailbox and error values.
    impl<T: crate::Config> MailboxCallbacks<<Pallet<T> as Messenger>::OutputError>
        for MailBoxCallbacks<T>
    where
        T::AccountId: Origin,
    {
        type Value = <Pallet<T> as Messenger>::MailboxedMessage;
        type BlockNumber = T::BlockNumber;

        type GetBlockNumber = GetBlockNumber<T>;
        type OnInsert = ();
        type OnRemove = ();
    }

    // ----

    // Below goes callbacks, used for waitlist algorithm.
    //
    // Note, that they are public like storage wrappers
    // only to be able to use as public trait's generics.

    // ----

    /// Callback function for getting actual block number.
    pub struct GetBlockNumber<T: crate::Config>(PhantomData<T>)
    where
        T::AccountId: Origin;

    // Callback trait implementation.
    impl<T: crate::Config> GetCallback<T::BlockNumber> for GetBlockNumber<T>
    where
        T::AccountId: Origin,
    {
        fn call() -> T::BlockNumber {
            T::CurrentBlockNumber::get()
        }
    }

    // ----

    /// Store of waitlist action's callbacks.
    pub struct WaitListCallbacks<T: crate::Config>(PhantomData<T>)
    where
        T::AccountId: Origin;

    // Callbacks store for waitlist trait implementation, over
    // specified (associated) types of waitlist and error values.
    impl<T: crate::Config> WaitlistCallbacks for WaitListCallbacks<T>
    where
        T::AccountId: Origin,
    {
        type Value = <Pallet<T> as Messenger>::WaitlistedMessage;
        type BlockNumber = T::BlockNumber;

        type GetBlockNumber = GetBlockNumber<T>;
        type OnInsert = ();
        type OnRemove = ();
    }

    // ----

    // Below goes final `Messenger` implementation for
    // Gear Messenger Pallet based on above generated
    // types and parameters.

    /// Message processing centralized behavior for
    /// Gear Messenger Pallet.
    ///
    /// See `gear_common::storage::Messenger` for
    /// complete documentation.
    impl<T: crate::Config> Messenger for Pallet<T>
    where
        T::AccountId: Origin,
    {
        type BlockNumber = T::BlockNumber;
        type Capacity = Capacity;
        type Error = Error<T>;
        type OutputError = DispatchError;

        type MailboxFirstKey = T::AccountId;
        type MailboxSecondKey = MessageId;
        type MailboxedMessage = StoredMessage;
        type QueuedDispatch = StoredDispatch;
        type WaitlistFirstKey = ProgramId;
        type WaitlistSecondKey = MessageId;
        type WaitlistedMessage = StoredDispatch;
        type DispatchStashKey = MessageId;

        type Sent = CounterImpl<Self::Capacity, SentWrap<T>>;

        type Dequeued = CounterImpl<Self::Capacity, DequeuedWrap<T>>;

        type QueueProcessing = TogglerImpl<QueueProcessingWrap<T>>;

        type Queue = QueueImpl<
            DequeueImpl<
                MessageId,
                Self::QueuedDispatch,
                Self::Error,
                HeadWrap<T>,
                TailWrap<T>,
                DispatchesWrap<T>,
                QueueCallbacks<T>,
            >,
            DispatchError,
            QueueKeyGen,
        >;

        type Mailbox = MailboxImpl<
            MailboxWrap<T>,
            Self::MailboxedMessage,
            Self::BlockNumber,
            Self::Error,
            DispatchError,
            MailBoxCallbacks<T>,
            MailboxKeyGen<T::AccountId>,
        >;

        type Waitlist = WaitlistImpl<
            WaitlistWrap<T>,
            Self::WaitlistedMessage,
            Self::BlockNumber,
            Self::Error,
            DispatchError,
            WaitListCallbacks<T>,
            WaitlistKeyGen,
        >;

        type DispatchStash = DispatchStashWrap<T>;
    }

    // Gear Messenger Pallet hooks.
    //
    // The logic of the pallet provides block-dependent logic
    // (amount of messages sent within the block, etc.), so we
    // have to reset the state per each block initialization.
    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
    where
        T::AccountId: Origin,
    {
        /// Block initialization.
        fn on_initialize(_bn: BlockNumberFor<T>) -> Weight {
            // Amount of weight used for initialization.
            let mut weight = Weight::zero();

            // Clear amount of messages sent.
            //
            // Removes value from storage. Single DB write.
            <Self as Messenger>::Sent::reset();
            weight += T::DbWeight::get().writes(1);

            // Clear amount of messages dequeued.
            //
            // Removes value from storage. Single DB write.
            <Self as Messenger>::Dequeued::reset();
            weight += T::DbWeight::get().writes(1);

            // Allow queue processing in this block.
            //
            // Puts value in storage. Single DB write.
            <Self as Messenger>::QueueProcessing::allow();
            weight += T::DbWeight::get().writes(1);

            weight
        }
    }
}
