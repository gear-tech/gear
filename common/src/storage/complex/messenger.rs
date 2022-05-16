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

use crate::storage::{
    complex::{Mailbox, MailboxError, Queue},
    complicated::{Counter, LinkedListError, Toggler},
    primitives::{Counted, IterableMap},
};
use core::fmt::Debug;

/// Message processing centralized behaviour.
pub trait Messenger {
    type Capacity;
    type Error: MailboxError + LinkedListError + Debug;
    type OutputError: From<Self::Error> + Debug;

    type MailboxFirstKey;
    type MailboxSecondKey;
    type MailboxedMessage;
    type QueuedDispatch;

    /// Amount of messages sent from outside.
    type Sent: Counter<Value = Self::Capacity>;

    /// Amount of messages dequeued.
    type Dequeued: Counter<Value = Self::Capacity>;

    /// Allowance of queue processing.
    type QueueProcessing: Toggler;

    /// Message queue store.
    type Queue: Queue<Value = Self::QueuedDispatch, Error = Self::Error, OutputError = Self::OutputError>
        + Counted<Length = Self::Capacity>
        + IterableMap<Result<Self::QueuedDispatch, Self::OutputError>>;

    /// Users mailbox store.
    type Mailbox: Mailbox<
        Key1 = Self::MailboxFirstKey,
        Key2 = Self::MailboxSecondKey,
        Value = Self::MailboxedMessage,
        Error = Self::Error,
        OutputError = Self::OutputError,
    >;

    fn reset() {
        Self::Sent::reset();
        Self::Dequeued::reset();
        Self::QueueProcessing::allow();
        Self::Queue::remove_all();
        Self::Mailbox::remove_all();
    }
}
