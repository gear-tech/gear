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

//! Outgoing message implementations.

use super::{
    stored::{BaseMailboxMessage, BaseQueueMessage, BaseStashMessage},
    BaseMessage, IncrementNonce, MessageKind, WithDestination, WithSource,
};
use crate::{
    buffer::Payload,
    ids::prelude::{ActorIdExt, ExternalActorMessagingData, MessageIdExt},
};
use gear_core_errors::ReplyCode;
use gprimitives::{ActorId, CodeId, MessageId};
use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// Ready-to-be-sent outgoing message.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Encode,
    Decode,
    MaxEncodedLen,
    TypeInfo,
    derive_more::Deref,
    derive_more::DerefMut,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct OutgoingMessage {
    #[deref]
    #[deref_mut]
    base: WithDestination<BaseMessage<Payload>>,

    gas: Option<u64>,
    details: OutgoingMessageDetails,
}

impl OutgoingMessage {
    /// Creates a new outgoing message with the given params.
    ///
    /// For testing purposes only.
    #[cfg(feature = "test-utils")]
    pub const fn new(
        base: BaseMessage<Payload>,
        destination: ActorId,
        gas: Option<u64>,
        details: OutgoingMessageDetails,
    ) -> Self {
        let base = WithDestination::new(base, destination);

        Self { base, gas, details }
    }

    // TODO (breathx/refactor(gear-core)): use &mut MessageContext here.
    /// Creates a new outgoing init message from an executable actor (Gear Program).
    #[allow(clippy::too_many_arguments)]
    pub fn init_from_executable(
        origin_message_id: MessageId,
        messaging_nonce: &mut IncrementNonce,
        code_id: CodeId,
        salt: &[u8],
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let destination = ActorId::generate_from_program(origin_message_id, code_id, salt);
        let message_id =
            MessageId::generate_outgoing(origin_message_id, messaging_nonce.fetch_inc());

        Self::_init(code_id, message_id, destination, payload, gas, value, delay)
    }

    /// Creates a new outgoing init message from an external actor.
    pub fn init_from_external(
        external_data: ExternalActorMessagingData,
        code_id: CodeId,
        salt: &[u8],
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let destination = ActorId::generate_from_user(code_id, salt);
        let message_id = external_data.generate_message_id();

        Self::_init(code_id, message_id, destination, payload, gas, value, delay)
    }

    /// Creates a new outgoing handle message from an executable actor (Gear Program).
    pub fn handle_from_executable(
        origin_message_id: MessageId,
        messaging_nonce: &mut IncrementNonce,
        destination: ActorId,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let message_id =
            MessageId::generate_outgoing(origin_message_id, messaging_nonce.fetch_inc());

        Self::_handle(message_id, destination, payload, gas, value, delay)
    }

    /// Creates a new outgoing handle message from an external actor.
    pub fn handle_from_external(
        external_data: ExternalActorMessagingData,
        destination: ActorId,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let message_id = external_data.generate_message_id();

        Self::_handle(message_id, destination, payload, gas, value, delay)
    }

    /// Creates a new outgoing reply message.
    ///
    /// For internal use only.
    ///
    /// Replies generation should be implemented on other messages types,
    /// if applicable.
    pub(crate) fn reply(
        to: MessageId,
        code: ReplyCode,
        destination: ActorId,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
    ) -> Self {
        let message_id = MessageId::generate_reply(to);
        let base = WithDestination::new(BaseMessage::new(message_id, payload, value), destination);
        let details = OutgoingMessageDetails::Reply { to, code };

        Self { base, gas, details }
    }

    /// Returns the gas limit for the message, if any.
    pub fn gas(&self) -> Option<u64> {
        self.gas
    }

    /// Returns the kind-specific details of the message.
    pub fn details(&self) -> OutgoingMessageDetails {
        self.details
    }

    /// Converts an outgoing message to a mailboxed one.
    ///
    /// Returns `None` if the message meets any of the following conditions:
    /// - It is not of the `Handle` kind.
    /// - It has a non-zero delay.
    pub fn try_into_mailbox(self) -> Option<BaseMailboxMessage<Payload>> {
        // TODO (breathx/refactor(gear-core)): consider to check force_queue flag.
        (self.details.as_kind() == MessageKind::Handle && self.details.delay() == 0)
            .then(|| BaseMailboxMessage::_new(self.base.into_inner().into_inner()))
    }

    /// Converts an outgoing message to a queued one.
    ///
    /// Returns `None` if the message has non-zero delay.
    pub fn try_into_queue(self, source: ActorId) -> Option<BaseQueueMessage<Payload>> {
        // TODO (breathx/refactor(gear-core)): consider to check force_queue flag.
        (self.details.delay() == 0).then(|| {
            let blank = WithSource::new(self.base.into_inner().into_inner(), source);
            let details = self.details.into();
            let history = None;

            BaseQueueMessage::_new(blank, details, history)
        })
    }

    /// Converts an outgoing message to a stashed one.
    ///
    /// Returns `None` if the message has zero delay.
    pub fn try_into_stash(self, source: ActorId) -> Option<BaseStashMessage<Payload>> {
        (self.details.delay() == 0).then(|| {
            let blank = WithSource::new(self.base.into_inner().into_inner(), source);
            // Replies always have zero delay, so they are unreachable here.
            let is_init = self.details.as_kind() == MessageKind::Init;

            BaseStashMessage::_new(blank, is_init)
        })
    }

    fn _init(
        code_id: CodeId,
        message_id: MessageId,
        destination: ActorId,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let base = WithDestination::new(BaseMessage::new(message_id, payload, value), destination);
        let details = OutgoingMessageDetails::Init { code_id, delay };

        Self { base, gas, details }
    }

    fn _handle(
        message_id: MessageId,
        destination: ActorId,
        payload: Payload,
        gas: Option<u64>,
        value: u128,
        delay: u32,
    ) -> Self {
        let base = WithDestination::new(BaseMessage::new(message_id, payload, value), destination);
        let details = OutgoingMessageDetails::Handle { delay };

        Self { base, gas, details }
    }
}

/// Kind-specific details for message sending.
///
/// This enum provides additional data required for executing messages based on
/// their kind, such as reply details or code ID to initialize a program.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, MaxEncodedLen, TypeInfo,
)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub enum OutgoingMessageDetails {
    /// Initialization message details.
    Init {
        /// Code ID to be used for creating a new program, which will
        /// subsequently be initialized by this message.
        code_id: CodeId,
        /// Delay for the message to be sent.
        delay: u32,
    },
    /// Handle message details.
    Handle {
        // TODO (breathx/refactor(gear-core)): deprecate JournalNote::StoreNewPrograms.
        // force_queue: bool, // OR to_program: bool,
        /// Delay for the message to be sent.
        delay: u32,
    },
    /// Reply message details.
    Reply {
        /// Message ID being replied to.
        to: MessageId,
        /// Reply code.
        code: ReplyCode,
    },
}

impl OutgoingMessageDetails {
    /// Returns the kind of message these details relate to.
    pub fn as_kind(&self) -> MessageKind {
        match self {
            Self::Init { .. } => MessageKind::Init,
            Self::Handle { .. } => MessageKind::Handle,
            Self::Reply { .. } => MessageKind::Reply,
        }
    }

    /// Returns the delay for the message to be sent.
    ///
    /// This is only applicable for `Init` and `Handle` message kinds,
    /// `Reply` messages are always sent immediately.
    pub fn delay(&self) -> u32 {
        match self {
            Self::Init { delay, .. } => *delay,
            Self::Handle { delay, .. } => *delay,
            Self::Reply { .. } => 0,
        }
    }
}
