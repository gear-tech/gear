// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

//! Module for mailbox implementation.
//!
//! Mailbox provides functionality of storing messages,
//! addressed to users.

use crate::storage::{
    Callback, CountedByKey, DoubleMapStorage, GetCallback, Interval, IterableByKeyMap, IterableMap,
    KeyFor,
};
use core::marker::PhantomData;

pub type ValueWithInterval<T, B> = (T, Interval<B>);

/// Represents mailbox managing logic.
pub trait Mailbox {
    /// First key type.
    type Key1;
    /// Second key type.
    type Key2;
    /// Stored values type.
    type Value;
    /// Block number type.
    ///
    /// Stored with `Self::Value`.
    type BlockNumber;
    /// Inner error type of mailbox storing algorithm.
    type Error: MailboxError;
    /// Output error type of the mailbox.
    type OutputError: From<Self::Error>;

    /// Returns bool, defining does first key's mailbox contain second key.
    fn contains(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    /// Inserts given value in mailbox.
    fn insert(value: Self::Value, bn: Self::BlockNumber) -> Result<(), Self::OutputError>;

    /// Removes and returns value from mailbox by given keys,
    /// if present, else returns error.
    fn remove(
        key1: Self::Key1,
        key2: Self::Key2,
    ) -> Result<ValueWithInterval<Self::Value, Self::BlockNumber>, Self::OutputError>;

    /// Removes all values from all key's mailboxes.
    fn clear();
}

/// Represents store of mailbox's action callbacks.
pub trait MailboxCallbacks<OutputError> {
    /// Callback relative type.
    ///
    /// This value should be the main item of mailbox,
    /// which uses this callbacks store.
    type Value;
    /// Callback relative type.
    ///
    /// This type represents block number of stored component in waitlist,
    /// which uses this callbacks store.
    type BlockNumber;

    /// Callback used for getting current block number.
    type GetBlockNumber: GetCallback<Self::BlockNumber>;
    /// Callback on success `insert`.
    type OnInsert: Callback<ValueWithInterval<Self::Value, Self::BlockNumber>>;
    /// Callback on success `remove`.
    type OnRemove: Callback<ValueWithInterval<Self::Value, Self::BlockNumber>>;
}

/// Represents mailbox error type.
///
/// Contains constructors for all existing errors.
pub trait MailboxError {
    /// Occurs when given value already exists in mailbox.
    fn duplicate_key() -> Self;

    /// Occurs when element wasn't found in storage.
    fn element_not_found() -> Self;
}

/// `Mailbox` implementation based on `DoubleMapStorage`.
///
/// Generic parameter `Error` requires `MailboxError` implementation.
/// Generic parameter `KeyGen` presents key generation for given values.
/// Generic parameter `Callbacks` presents actions for success operations
/// over mailbox.
pub struct MailboxImpl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen>(
    PhantomData<(T, Error, OutputError, Callbacks, KeyGen)>,
)
where
    T: DoubleMapStorage<Value = ValueWithInterval<Value, BlockNumber>>,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = Value, BlockNumber = BlockNumber>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = Value>;

// Implementation of `Mailbox` for `MailboxImpl`.
impl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen> Mailbox
    for MailboxImpl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage<Value = ValueWithInterval<Value, BlockNumber>>,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = Value, BlockNumber = BlockNumber>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = Value>,
{
    type Key1 = T::Key1;
    type Key2 = T::Key2;
    type Value = Value;
    type BlockNumber = BlockNumber;
    type Error = Error;
    type OutputError = OutputError;

    fn contains(user_id: &Self::Key1, message_id: &Self::Key2) -> bool {
        T::contains_keys(user_id, message_id)
    }

    fn insert(
        message: Self::Value,
        scheduled_at: Self::BlockNumber,
    ) -> Result<(), Self::OutputError> {
        let (key1, key2) = KeyGen::key_for(&message);

        if Self::contains(&key1, &key2) {
            return Err(Self::Error::duplicate_key().into());
        }

        let block_number = Callbacks::GetBlockNumber::call();
        let message_with_bn = (
            message,
            Interval {
                start: block_number,
                finish: scheduled_at,
            },
        );

        Callbacks::OnInsert::call(&message_with_bn);
        T::insert(key1, key2, message_with_bn);
        Ok(())
    }

    fn remove(
        user_id: Self::Key1,
        message_id: Self::Key2,
    ) -> Result<ValueWithInterval<Self::Value, Self::BlockNumber>, Self::OutputError> {
        if let Some(message_with_bn) = T::take(user_id, message_id) {
            Callbacks::OnRemove::call(&message_with_bn);
            Ok(message_with_bn)
        } else {
            Err(Self::Error::element_not_found().into())
        }
    }

    fn clear() {
        T::clear()
    }
}

// Implementation of `CountedByKey` trait for `MailboxImpl` in case,
// when inner `DoubleMapStorage` implements `CountedByKey`.
impl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen> CountedByKey
    for MailboxImpl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage<Value = ValueWithInterval<Value, BlockNumber>>
        + CountedByKey<Key = T::Key1>,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = Value, BlockNumber = BlockNumber>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = Value>,
{
    type Key = T::Key1;
    type Length = T::Length;

    fn len(key: &Self::Key) -> Self::Length {
        T::len(key)
    }
}

// Implementation of `IterableByKeyMap` trait for `MailboxImpl` in case,
// when inner `DoubleMapStorage` implements `IterableByKeyMap`.
impl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen> IterableByKeyMap<T::Value>
    for MailboxImpl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage<Value = ValueWithInterval<Value, BlockNumber>>
        + IterableByKeyMap<T::Value, Key = T::Key1>,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = Value, BlockNumber = BlockNumber>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = Value>,
{
    type Key = T::Key1;
    type DrainIter = T::DrainIter;
    type Iter = T::Iter;

    fn drain_key(key: Self::Key) -> Self::DrainIter {
        T::drain_key(key)
    }

    fn iter_key(key: Self::Key) -> Self::Iter {
        T::iter_key(key)
    }
}

// Implementation of `IterableMap` trait for `MailboxImpl` in case,
// when inner `DoubleMapStorage` implements `IterableMap`.
impl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen> IterableMap<T::Value>
    for MailboxImpl<T, Value, BlockNumber, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage<Value = ValueWithInterval<Value, BlockNumber>> + IterableMap<T::Value>,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = Value, BlockNumber = BlockNumber>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = Value>,
{
    type DrainIter = T::DrainIter;
    type Iter = T::Iter;

    fn drain() -> Self::DrainIter {
        T::drain()
    }

    fn iter() -> Self::Iter {
        T::iter()
    }
}
