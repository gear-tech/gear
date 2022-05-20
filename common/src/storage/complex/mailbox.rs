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

use crate::storage::primitives::{Callback, DoubleMapStorage, FallibleCallback, KeyFor};
use core::marker::PhantomData;

/// Represents mailbox managing logic.
pub trait Mailbox {
    /// First key type.
    type Key1;
    /// Second key type.
    type Key2;
    /// Stored values type.
    type Value;
    /// Inner error type of mailbox storing algorithm.
    type Error: MailboxError;
    /// Output error type of the mailbox.
    type OutputError: From<Self::Error>;

    /// Returns `Vec` of values from mailbox of given key.
    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value>;

    /// Returns bool, defining does first key's mailbox contain second key.
    fn contains(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    /// Returns amount of values in mailbox of given key.
    fn count_of(key: &Self::Key1) -> usize;

    /// Inserts given value in mailbox.
    fn insert(value: Self::Value) -> Result<(), Self::OutputError>;

    /// Returns bool, defining if given key's mailbox is empty.
    fn is_empty(key: &Self::Key1) -> bool {
        Self::count_of(key) == 0
    }

    /// Removes and returns value from mailbox by given keys,
    /// if present, else returns error.
    fn remove(key1: Self::Key1, key2: Self::Key2) -> Result<Self::Value, Self::OutputError>;

    /// Removes all values from all key's mailboxes.
    fn remove_all();
}

/// Represents store of mailbox's action callbacks.
pub trait MailboxCallbacks<OutputError> {
    /// Callback relative type.
    ///
    /// This value should be the main item of mailbox,
    /// which uses this callbacks store.
    type Value;

    /// Callback on success `insert`.
    type OnInsert: Callback<Self::Value>;
    /// Callback on success `remove`.
    type OnRemove: FallibleCallback<Self::Value, Error = OutputError>;
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
pub struct MailboxImpl<T, Error, OutputError, Callbacks, KeyGen>(
    PhantomData<(T, Error, OutputError, Callbacks, KeyGen)>,
)
where
    T: DoubleMapStorage,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = T::Value>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = T::Value>;

// Implementation of `Mailbox` for `MailboxImpl`.
impl<T, Error, OutputError, Callbacks, KeyGen> Mailbox
    for MailboxImpl<T, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = T::Value>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = T::Value>,
{
    type Key1 = T::Key1;
    type Key2 = T::Key2;
    type Value = T::Value;
    type Error = Error;
    type OutputError = OutputError;

    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value> {
        T::collect_of(key)
    }

    fn contains(user_id: &Self::Key1, message_id: &Self::Key2) -> bool {
        T::contains_keys(user_id, message_id)
    }

    fn count_of(user_id: &Self::Key1) -> usize {
        T::count_of(user_id)
    }

    fn insert(message: Self::Value) -> Result<(), Self::OutputError> {
        let (key1, key2) = KeyGen::key_for(&message);

        if Self::contains(&key1, &key2) {
            return Err(Self::Error::duplicate_key().into());
        }

        Callbacks::OnInsert::call(&message);
        T::insert(key1, key2, message);
        Ok(())
    }

    fn remove(
        user_id: Self::Key1,
        message_id: Self::Key2,
    ) -> Result<Self::Value, Self::OutputError> {
        if let Some(msg) = T::take(user_id, message_id) {
            Callbacks::OnRemove::call(&msg)?;
            Ok(msg)
        } else {
            Err(Self::Error::element_not_found().into())
        }
    }

    fn remove_all() {
        T::remove_all()
    }
}

// Extra methods for soft use of `MailboxImpl`.
impl<T, Error, OutputError, Callbacks, KeyGen> MailboxImpl<T, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = T::Value>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = T::Value>,
{
    /// Returns mailbox of specified user (first key).
    pub fn of(user_id: T::Key1) -> UserMailbox<Self> {
        UserMailbox(user_id, PhantomData)
    }
}

/// Represents wrapper over `Mailbox` to work with specified user's mailbox.
///
/// Can be only constructed by `MailboxImpl`.
pub struct UserMailbox<MB: Mailbox>(MB::Key1, PhantomData<MB>);

// Soft methods of `UserMailbox`.
impl<MB: Mailbox> UserMailbox<MB> {
    /// Returns `Vec` of values from current user's mailbox.
    pub fn collect(self) -> crate::Vec<MB::Value> {
        MB::collect_of(self.0)
    }

    /// Returns bool, defining does current user's mailbox
    /// contain given second key.
    pub fn contains(&self, message_id: &MB::Key2) -> bool {
        MB::contains(&self.0, message_id)
    }

    /// Returns bool, defining does current user's mailbox
    /// contain no elements.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns amount of messages in current user's mailbox.
    pub fn len(&self) -> usize {
        MB::count_of(&self.0)
    }

    /// Removes and returns value from current user's mailbox
    /// by given second key, if present, else returns error.
    pub fn remove(self, message_id: MB::Key2) -> Result<MB::Value, MB::OutputError> {
        MB::remove(self.0, message_id)
    }
}
