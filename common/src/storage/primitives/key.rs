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

use core::marker::PhantomData;

use crate::Origin;
use gear_core::{
    ids::MessageId,
    message::{StoredDispatch, StoredMessage},
};

pub trait KeyFor {
    type Key;
    type Value;

    fn key_for(value: &Self::Value) -> Self::Key;
}

pub struct QueueKeyGen;

impl KeyFor for QueueKeyGen {
    type Key = MessageId;
    type Value = StoredDispatch;

    fn key_for(value: &Self::Value) -> Self::Key {
        value.id()
    }
}

pub struct MailboxKeyGen<T>(PhantomData<T>);

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
