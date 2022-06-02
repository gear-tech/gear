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

use codec::{Decode, Encode};
use gear_core::ids::MessageId;
use primitive_types::H256;
use scale_info::TypeInfo;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum Entry {
    Init,
    Handle,
    Reply(MessageId),
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum DispatchStatus {
    Success,
    Failed,
    NotExecuted,
}

pub trait RuntimeReason: Sized {
    fn into_reason<S: SystemReason>(self) -> Reason<Self, S> {
        Reason::Runtime(self)
    }
}

impl RuntimeReason for () {}

pub trait SystemReason: Sized {
    fn into_reason<R: RuntimeReason>(self) -> Reason<R, Self> {
        Reason::System(self)
    }
}

impl SystemReason for () {}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum Reason<R: RuntimeReason, S: SystemReason> {
    Runtime(R),
    System(S),
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum MessageWaitedRuntimeReason {
    WaitCalled,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum MessageWaitedSystemReason {
    DidNotFinishInit,
}

pub type MessageWaitedReason = Reason<MessageWaitedRuntimeReason, MessageWaitedSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum MessageWakenRuntimeReason {
    WakeCalled,
    TimeoutBecome,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum MessageWakenSystemReason {
    FailedInit,
    OutOfRent,
}

pub type MessageWakenReason = Reason<MessageWakenRuntimeReason, MessageWakenSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum CodeChangeKind<BlockNumber> {
    Active { expiration: Option<BlockNumber> },

    Inactive,

    Reinstrumented,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
pub enum UserMessageReadRuntimeReason {
    Replied,
    Claimed,
}

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
pub enum UserMessageReadSystemReason {
    OutOfRent,
}

pub type UserMessageReadReason = Reason<UserMessageReadRuntimeReason, UserMessageReadSystemReason>;

#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
pub enum ProgramChangeKind<BlockNumber> {
    Active {
        expiration: BlockNumber,
    },

    Inactive,

    Paused {
        code_hash: H256,
        memory_hash: H256,
        waitlist_hash: H256,
    },

    StateChanged,
}
