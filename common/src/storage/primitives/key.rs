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

//! Module for map's key generation primitives.
//!
//! Key generator primitives might be used to prevent
//! manual specifying of the key for cases,
//! when data stored in map.

use crate::Origin;
use core::marker::PhantomData;
use gear_core::{
    ids::{MessageId, ProgramId},
    message::{StoredDispatch, StoredMessage},
};

/// Represents logic of providing key as specified
/// (associated) type for given as specified
/// (associated) type value by reference.
pub trait KeyFor {
    /// Generated key type.
    type Key;
    /// Value over which key should be generated type.
    type Value;

    /// Generates key for given by reference Value.
    fn key_for(value: &Self::Value) -> Self::Key;
}

/// Key generator for `gear`'s mailbox implementation.
pub struct MailboxKeyGen<T>(PhantomData<T>);

// `MailboxKeyGen` stores `StoredMessage` under it's
// `MessageId` id parameter and the generic `T: Origin`
// (represented with `Substrate`'s 32-byte `AccountId`)
// destination parameter.
impl<T: Origin> KeyFor for MailboxKeyGen<T> {
    type Key = (T, MessageId);
    type Value = StoredMessage;

    fn key_for(value: &Self::Value) -> Self::Key {
        (
            T::from_origin(value.destination().into_origin()),
            value.id(),
        )
    }
}

/// Key generator for `gear`'s message queue implementation.
pub struct QueueKeyGen;

// `QueueKeyGen` stores `StoredDispatch` under
// it's `MessageId` id parameter.
impl KeyFor for QueueKeyGen {
    type Key = MessageId;
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        value.id()
    }
}

/// Key generator for `gear`'s waitlist implementation.
pub struct WaitlistKeyGen;

// `WaitlistKeyGen` stores `StoredDispatch` under it's destination
// `ProgramId` and `MessageId` id parameters.
impl KeyFor for WaitlistKeyGen {
    type Key = (ProgramId, MessageId);
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        (value.destination(), value.id())
    }
}
