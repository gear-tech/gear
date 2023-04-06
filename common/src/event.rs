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

//! Gear events additional data.
//!
//! This module contains components for depositing proper
//! and extensive data about actions happen.

use frame_support::{
    codec::{self, Decode, Encode},
    scale_info::{self, TypeInfo},
};
use gear_core::{ids::MessageId, message::MessageWaitedType};
use primitive_types::H256;

/// Programs entry for messages.
///
/// Same as `gear_core::message::DispatchKind`,
/// but with additional info about reply.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum MessageEntry {
    /// Init entry point.
    Init,
    /// Handle entry point.
    Handle,
    /// Handle reply entry point.
    Reply(MessageId),
    /// System signal entry point.
    Signal,
}

/// Status of dispatch dequeue and execution.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum DispatchStatus {
    /// Dispatch was dequeued and succeed with execution.
    Success,
    /// Dispatch was dequeued and failed its execution.
    Failed,
    /// Dispatch was dequeued and wasn't executed.
    /// Occurs if actor no longer exists.
    NotExecuted,
}

/// Behavior of types, which represent runtime reasons for some chain actions.
pub trait RuntimeReason: Sized {
    /// Converter into composite reason type: not only runtime, but system also.
    fn into_reason<S: SystemReason>(self) -> Reason<Self, S> {
        Reason::Runtime(self)
    }
}

// Empty implementation for `()` to skip requirements.
impl RuntimeReason for () {}

/// Behavior of types, which represent system reasons for some chain actions.
pub trait SystemReason: Sized {
    /// Converter into composite reason type: not only system, but runtime also.
    fn into_reason<R: RuntimeReason>(self) -> Reason<R, Self> {
        Reason::System(self)
    }
}

// Empty implementation for `()` to skip requirements.
impl SystemReason for () {}

/// Composite reason type for any action happened on chain.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum Reason<R: RuntimeReason, S: SystemReason> {
    /// Runtime reason variant.
    ///
    /// This means that actor explicitly forced some action,
    /// which this reason explains.
    Runtime(R),
    /// System reason variant.
    ///
    /// This means that system automatically forced some action,
    /// which this reason explains.
    System(S),
}

/// Runtime reason for messages waiting.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum MessageWaitedRuntimeReason {
    /// Program called `gr_wait` while executing message.
    WaitCalled,
    /// Program called `gr_wait_for` while executing message.
    WaitForCalled,
    /// Program called `gr_wait_up_to` with insufficient gas for full
    /// duration while executing message.
    WaitUpToCalled,
    /// Program called `gr_wait_up_to` with enough gas for full duration
    /// storing while executing message.
    WaitUpToCalledFull,
}

impl From<MessageWaitedType> for MessageWaitedRuntimeReason {
    fn from(src: MessageWaitedType) -> Self {
        match src {
            MessageWaitedType::Wait => MessageWaitedRuntimeReason::WaitCalled,
            MessageWaitedType::WaitFor => MessageWaitedRuntimeReason::WaitForCalled,
            MessageWaitedType::WaitUpTo => MessageWaitedRuntimeReason::WaitUpToCalled,
            MessageWaitedType::WaitUpToFull => MessageWaitedRuntimeReason::WaitUpToCalledFull,
        }
    }
}

/// System reason for messages waiting.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum MessageWaitedSystemReason {
    /// Program hadn't finished initialization and can not
    /// process received message yet.
    ProgramIsNotInitialized,
}

/// Composite reason for messages waiting.
pub type MessageWaitedReason = Reason<MessageWaitedRuntimeReason, MessageWaitedSystemReason>;

/// Runtime reason for messages waking.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum MessageWokenRuntimeReason {
    /// Program called `gr_wake` with corresponding message id.
    WakeCalled,
}

/// System reason for messages waking.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum MessageWokenSystemReason {
    /// Program had finished initialization.
    ///
    /// Note that this variant doesn't contain info
    /// about initialization success or failure.
    ProgramGotInitialized,
    /// Specified by program timeout for waking has come (see #349).
    TimeoutHasCome,
    /// Message can no longer pay rent for holding in storage (see #646).
    OutOfRent,
}

/// Composite reason for messages waking.
pub type MessageWokenReason = Reason<MessageWokenRuntimeReason, MessageWokenSystemReason>;

/// Type of changes applied to code in storage.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum CodeChangeKind<BlockNumber> {
    /// Code become active and ready for use.
    ///
    /// Appear when new code created or expiration block number updated.
    ///
    /// Expiration block number presents block number when this code become
    /// inactive due to losing ability to pay rent for holding.
    /// Equals `None` if stores free (some program relays on it, see #646).
    Active { expiration: Option<BlockNumber> },

    /// Code become inactive and can no longer be used.
    Inactive,

    /// Code was reinstrumented.
    Reinstrumented,
}

/// Runtime reason for messages reading from `Mailbox`.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum UserMessageReadRuntimeReason {
    /// Message was replied by user.
    MessageReplied,
    /// Message was claimed by user.
    MessageClaimed,
}

/// System reason for messages reading from `Mailbox`.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo, SystemReason)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum UserMessageReadSystemReason {
    /// Message can no longer pay rent for holding in storage (see #646).
    OutOfRent,
}

/// Composite reason for messages reading from `Mailbox`.
pub type UserMessageReadReason = Reason<UserMessageReadRuntimeReason, UserMessageReadSystemReason>;

/// Type of changes applied to program in storage.
#[derive(Debug, Encode, Decode, Clone, PartialEq, Eq, TypeInfo)]
#[codec(crate = codec)]
#[scale_info(crate = scale_info)]
pub enum ProgramChangeKind<BlockNumber> {
    /// Active status achieved.
    ///
    /// Occurs when new program created, paused program was resumed
    /// or expiration block number updated.
    ///
    /// Expiration block number presents block number when this program become
    /// paused due to losing ability to pay rent for holding.
    Active { expiration: BlockNumber },

    // TODO: consider about addition expiration block number (issue #1014).
    /// Program become inactive forever due to `gr_exit` call.
    Inactive,

    // TODO: consider about addition expiration block number (issue #1014).
    /// Paused status.
    ///
    /// Program is no longer available for interaction, but can be
    /// resumed by paying rent and giving whole data related to it.
    Paused {
        /// Code hash the program relates to.
        code_hash: H256,
        /// Hash of memory pages of the program.
        memory_hash: H256,
        /// Waitlist hash addressed to the program.
        waitlist_hash: H256,
    },
}
