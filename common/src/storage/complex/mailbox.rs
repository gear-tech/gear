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

use crate::storage::primitives::{Callback, DoubleMapStorage, FallibleCallback, KeyFor};
use core::marker::PhantomData;

pub trait MailboxCallbacks<OutputError> {
    type Value;

    type OnInsert: Callback<Self::Value>;
    type OnRemove: FallibleCallback<Self::Value, Error = OutputError>;
}

pub trait MailboxError {
    fn duplicate_key() -> Self;

    fn element_not_found() -> Self;
}

pub trait Mailbox {
    type Key1;
    type Key2;
    type Value;
    type Error: MailboxError;
    type OutputError: From<Self::Error>;

    fn contains(key1: &Self::Key1, key2: &Self::Key2) -> bool;

    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value>;

    fn count_of(key: &Self::Key1) -> usize;

    fn insert(value: Self::Value) -> Result<(), Self::OutputError>;

    fn is_empty(user_id: &Self::Key1) -> bool {
        Self::count_of(user_id) == 0
    }

    fn remove(key1: Self::Key1, key2: Self::Key2) -> Result<Self::Value, Self::OutputError>;

    fn remove_all();
}

pub struct MailboxImpl<T, Error, OutputError, Callbacks, KeyGen>(
    PhantomData<(T, Error, OutputError, Callbacks, KeyGen)>,
)
where
    T: DoubleMapStorage,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = T::Value>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = T::Value>;

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

    fn contains(user_id: &Self::Key1, message_id: &Self::Key2) -> bool {
        T::contains_keys(user_id, message_id)
    }

    fn collect_of(key: Self::Key1) -> crate::Vec<Self::Value> {
        T::collect_of(key)
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

impl<T, Error, OutputError, Callbacks, KeyGen> MailboxImpl<T, Error, OutputError, Callbacks, KeyGen>
where
    T: DoubleMapStorage,
    Error: MailboxError,
    OutputError: From<Error>,
    Callbacks: MailboxCallbacks<OutputError, Value = T::Value>,
    KeyGen: KeyFor<Key = (T::Key1, T::Key2), Value = T::Value>,
{
    pub fn of(user_id: T::Key1) -> UserMailbox<Self> {
        UserMailbox(user_id, PhantomData)
    }
}

pub struct UserMailbox<MB: Mailbox>(MB::Key1, PhantomData<MB>);

impl<MB: Mailbox> UserMailbox<MB> {
    pub fn len(&self) -> usize {
        MB::count_of(&self.0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, message_id: &MB::Key2) -> bool {
        MB::contains(&self.0, message_id)
    }

    pub fn collect(self) -> crate::Vec<MB::Value> {
        MB::collect_of(self.0)
    }

    pub fn remove(self, message_id: MB::Key2) -> Result<MB::Value, MB::OutputError> {
        MB::remove(self.0, message_id)
    }
}
