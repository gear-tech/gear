// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

//! # Messages in the Gear Protocol
//!
//! Messages are the core component of the Gear Protocol and the only option
//! for actors to communicate within the system.
//!
//! Each message has its unique *message ID*, *payload*, and attached *value*,
//! which represents the token of the environment the Gear Protocol is running in.
//!
//! Depending on the context in which we operate with the message, it may have other
//! parameters, such as *source* actor ID, *destination* actor ID, etc.
//!
//! ## Message Kinds
//!
//! Messages sent to Gear Programs will be handled by Gear Programs
//! in dedicated WASM exports, if they exist:
//! - `init`
//! - `handle`
//! - `handle_reply`
//! - `handle_signal`
//!
//! These exports define basic message kinds. All of the kinds have their own
//! unique purpose and processing specifications.
//!
//! ### Common Behavior
//!
//! Despite the fact that all messages differ in their concept, they share some
//! common behavior.
//!
//! #### Processing
//!
//! A message is considered completely processed by the program if no other entries,
//! as well as state changes, can be made. At this point, the message is gone.
//!
//! Each message, at the moment of execution, can be interrupted by the program
//! using wait syscalls. This puts the message on a waitlist, postponing its
//! execution until the message is woken. This can be done by the system
//! according to the wait deadline inside the waitlist or by the program itself,
//! calling the wake syscall with the ID of the waited message.
//!
//! If there is no export dedicated to the message, execution is considered successful
//! without any actual execution.
//!
//! #### State Revert *on Failure*
//!
//! Any state changes made are reverted to the moment before the current execution.
//!
//! #### Value Return *on Failure*
//!
//! If it's the first-time execution of the message, meaning that the message has
//! never waited before, the value will be returned to the caller with
//! an error reply, if any is sent. Otherwise, it will be kept by the program.
//!
//! #### Illustrative Example
//!
//! Let's say there is a program `Counter` that has a state variable `count = 0`.
//!
//! In this example, messages are of the handle kind (see the Handle section).
//!
//! * Message A is sent to `Counter` with a non-zero value.
//!
//! It increments `count` by 1, then calls panic.
//!
//! In that case:
//!     - Message A is completely processed and gone.
//!     - `count` will be reverted to 0.
//!     - The attached value will be returned with an error reply sent.
//!
//! * Message B is sent to `Counter` with a non-zero value.
//!
//! It increments `count` by 1, then calls wait.
//!
//! In that case:
//!     - Message B is executed for the first time and put on the waitlist. No reply will be
//!       sent yet, since the message is interrupted and not completely processed.
//!     - `count` will be set to 1.
//!     - The attached value from now on belongs to the `Counter` program.
//!
//! Then, Message B leaves the waitlist by timeout or `Counter` calls wake on it.
//!
//! It increments `count` by 1, then calls panic.
//!
//! So what happens:
//!     - Message B is executed a second time and now is completely processed and gone forever.
//!     - `count` will be reverted to 1.
//!     - No value will be returned with the error reply.
//!
//! ### Init
//!
//! The `init` message is used to initialize a newly created program.
//!
//! * This message is handled by the `init` export.
//!
//! * It is only created for each program once.
//!
//! * The init message is always replied to.
//!
//! ### Handle
//!
//! The `handle` messages are the main kind of messages for actor communication.
//!
//! * These messages are handled by the `handle` export.
//!
//! * Handle messages are always replied to.
//!
//! ### Reply
//!
//! The `reply` messages are a specific kind of message directed to an actor
//! as a response to a message it sent previously.
//!
//! * These messages are handled by the `handle_reply` export.
//!
//! * Replies are conceptually the last element of some specific messaging
//!   sequence between actors, meaning that the original message
//!   is fully processed.
//!
//! * There cannot be several replies to the same message.
//!   The reply ID can be pre-calculated.
//!
//! * Replies cannot be sent until the message is completely processed.
//!   An attempt to send a reply and then interrupt will fail.
//!
//! * Replies have specific additional data:
//!     - The message ID they reply to.
//!     - The reply code.
//!
//!   The reply code constitutes short but comprehensive and sufficient information
//!   about the completion and outcome of the previously sent message.
//!   For details, see [`ReplyCode`](gear_core_errors::ReplyCode).
//!
//! * For messages that are supposed to always receive replies, if it wasn't
//!   sent by the program by the time it's completely processed with
//!   a successful outcome, the system will send an auto-reply with an
//!   empty payload and zero value.
//!
//! * Replies may be classified by their outcome:
//!     - Success replies:
//!         - Auto-replies, sent by the system.
//!         - Manual replies, sent by the program itself.
//!     - Error replies.
//!
//! * The payload of the error reply depends on the outcome:
//!     - In cases of a userspace panic, it will contain the given panic bytes.
//!     - In cases of program exit, it will contain the inheritor actor ID.
//!
//! * A reply message is never replied to.
//!   An attempt to send a reply will fail.
//!
//! ### Signal
//!
//! The `signal` messages are sent by the system to notify a Gear Program about
//! its other messages failing to complete processing.
//!
//! * These messages are handled by the `handle_signal` export.
//!
//! * Signals are intended to help programs recover some partially mutated state.
//!
//! * There cannot be several replies to the same message.
//!
//! * Signals are only sent to messages that made a `system_reservation`.
//!
//! * Signals have specific additional data:
//!     - The message ID the signal is sent to (about).
//!     - The signal code.
//!
//!   The signal code explains the reason for the signal creation.
//!   For details, see [`SignalCode`](gear_core_errors::SignalCode).
//!
//! * Signals are sent in the following cases:
//!     - A message is removed from the waitlist due to being out of rent.
//!     - A message fails due to an actual execution error.
//!
//! * Signals are never sent:
//!     - If no system reservation is provided.
//!     - To the following message kinds:
//!         - Init messages.
//!         - Error replies.
//!         - Other signals.
//!
//! * Signals are never replied to.
//!   An attempt to send a reply will fail.
//!
//! ## Message's Lifecycle
//!
//! Conceptually, each message sent by the actors, at any moment in time,
//! is represented by one of the following types:
//!
//! * `OutgoingMessage` - a newly created message that is ready to be sent,
//!   meaning it will be put into one of the storages.
//!
//! * `ExecutableMessage` - a message taken from the queue that is ready
//!   to be executed by an executable actor - a Gear Program.
//!
//! * A message stored in one of the following storages:
//!     * In `Mailbox` - an abstract storage for messages to external actors.
//!
//!       These messages are waiting for external actor actions,
//!       such as claiming or replying.
//!
//!       `Mailbox` only contains messages:
//!         - Of the `Handle` kind only.
//!         - That are sent to an external actor.
//!         - That have never been and won't be executed: have no execution history.
//!
//!     * In `Queue` - an abstract storage for messages to executable actors.
//!
//!       These messages are queued for execution by the actor.
//!
//!       `Queue` contains messages:
//!         - Of all kinds.
//!         - That are sent to an executable actor.
//!
//!     * In `Stash` - an abstract storage for messages sent with a delay.
//!
//!       These messages are waiting for the system to append them
//!       into the `Queue` or `Mailbox` at the right time.
//!
//!       `Stash` only contains messages:
//!         - Of the `Init` or `Handle` kinds only: only these may be sent with a delay.
//!         - That have never been executed: have no execution history.
//!
//!     * In `Waitlist` - an abstract storage for interrupted messages.
//!
//!       These messages are waiting for the system or the executable actor
//!       to wake them up, which will put them back into the `Queue`.
//!
//!       `Waitlist` contains messages:
//!         - Of all kinds.
//!         - That are sent to an executable actor.
//!         - That have been executed and interrupted: have execution history.
//!
//! This lifecycle can be represented as follows:
//! ```text
//! Outgoing
//! ├-> Stash
//! |   ├───────────┐
//! ├-> Mailbox     |
//! |   └-> [end]   v
//! └──-──────────> Queue <─-─- Waitlist <──┐
//!                 |           └-> [end]   |
//!                 └─> Executable          |
//!                     ├-> [end]           |
//!                     └───────────────────┘
//! ```

// TODO (breathx/refactor(gear-core)): impl Display for all structs.
// TODO (breathx/refactor(gear-core)): consider do not send signal on first execution.
// TODO (breathx/refactor(gear-core)): add docs references to all named types.

use gprimitives::MessageId;
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

pub use executable::{ExecutableMessage, ExecutableMessageDetails, ExecutionHistory};
pub use outgoing::{OutgoingMessage, OutgoingMessageDetails};
pub use utils::{
    IncrementNonce, WithDestination, WithId, WithSource, WrapWithDestination, WrapWithId,
    WrapWithSource,
};

mod executable;
mod outgoing;
mod utils;

pub mod stored;

/// Base message.
///
/// Each message has its own ID, payload (represented with some type), and value.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::From,
    derive_more::Into,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BaseMessage<P>(WithId<BlankMessage<P>>);

impl<P> BaseMessage<P> {
    /// Creates a new base message with the given payload, value and message ID.
    pub const fn new(id: MessageId, payload: P, value: u128) -> Self {
        Self::from_blank(id, BlankMessage::new(payload, value))
    }

    /// Creates a new base message with the given blank message and message ID.
    pub const fn from_blank(id: MessageId, blank: BlankMessage<P>) -> Self {
        Self(WithId::new(blank, id))
    }

    /// Converts the payload type using the provided function.
    pub fn convert<U>(self, f: impl FnOnce(P) -> U) -> BaseMessage<U> {
        let (blank, id) = self.0.into_parts();

        BaseMessage(WithId::new(blank.convert(f), id))
    }

    /// Decomposes the base message into the blank message.
    pub fn into_inner(self) -> BlankMessage<P> {
        self.0.into_inner()
    }

    /// Decomposes the base message into its blank message and ID.
    pub fn into_parts(self) -> (BlankMessage<P>, MessageId) {
        self.0.into_parts()
    }
}

impl<P> WrapWithDestination for BaseMessage<P> {}
impl<P> WrapWithSource for BaseMessage<P> {}

/// Ordinary non-unique blank message.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct BlankMessage<P> {
    payload: P,
    #[codec(compact)]
    value: u128,
}

impl<P> BlankMessage<P> {
    /// Creates a new blank message with the given payload and value.
    pub const fn new(payload: P, value: u128) -> Self {
        Self { payload, value }
    }

    /// Returns a reference to the payload of the message.
    pub fn payload(&self) -> &P {
        &self.payload
    }

    /// Returns a reference to the payload of the message as a byte slice.
    pub fn payload_bytes(&self) -> &[u8]
    where
        P: AsRef<[u8]>,
    {
        self.payload.as_ref()
    }

    /// Returns the value of the message.
    pub fn value(&self) -> u128 {
        self.value
    }

    /// Converts the payload type using the provided function.
    pub fn convert<U>(self, f: impl FnOnce(P) -> U) -> BlankMessage<U> {
        BlankMessage {
            payload: f(self.payload),
            value: self.value,
        }
    }

    /// Decomposes the blank message into its payload and value.
    pub fn into_parts(self) -> (P, u128) {
        (self.payload, self.value)
    }
}

impl<P> WrapWithDestination for BlankMessage<P> {}
impl<P> WrapWithId for BlankMessage<P> {}
impl<P> WrapWithSource for BlankMessage<P> {}

/// Enumeration of the different kinds of messages in the Gear Protocol.
///
/// Each kind differs by its handler and overall semantics, purpose, and behavior.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo, derive_more::Display,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum MessageKind {
    /// Initialization message.
    #[display("init")]
    Init,
    /// Regular message.
    #[display("handle")]
    Handle,
    /// Reply message.
    #[display("reply")]
    Reply,
    /// System signal message.
    #[display("signal")]
    Signal,
}

impl MessageKind {
    /// Returns a boolean indicating whether the message should have a reply.
    pub fn is_repliable(&self) -> bool {
        matches!(self, Self::Init | Self::Handle)
    }
}
