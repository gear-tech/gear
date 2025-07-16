// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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

use crate::{
    buffer::Payload,
    ids::{ActorId, MessageId, ReservationId, prelude::*},
    message::{
        Dispatch, HandleMessage, HandlePacket, IncomingMessage, InitMessage, InitPacket,
        ReplyMessage, ReplyPacket,
    },
    reservation::{GasReserver, ReservationNonce},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_core_errors::{ExecutionError, ExtError, MessageError as Error, MessageError};
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

use super::{DispatchKind, IncomingDispatch, Packet};

/// Context settings.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContextSettings {
    /// Fee for sending message.
    pub sending_fee: u64,
    /// Fee for sending scheduled message.
    pub scheduled_sending_fee: u64,
    /// Fee for calling wait.
    pub waiting_fee: u64,
    /// Fee for waking messages.
    pub waking_fee: u64,
    /// Fee for creating reservation.
    pub reservation_fee: u64,
    /// Limit of outgoing messages, that program can send in current message processing.
    pub outgoing_limit: u32,
    /// Limit of bytes in outgoing messages during current execution.
    pub outgoing_bytes_limit: u32,
}

impl ContextSettings {
    /// Returns default settings with specified outgoing messages limits.
    pub fn with_outgoing_limits(outgoing_limit: u32, outgoing_bytes_limit: u32) -> Self {
        Self {
            outgoing_limit,
            outgoing_bytes_limit,
            ..Default::default()
        }
    }
}

/// Dispatch or message with additional information.
pub type OutgoingMessageInfo<T> = (T, u32, Option<ReservationId>);
pub type OutgoingMessageInfoNoDelay<T> = (T, Option<ReservationId>);

/// Context outcome dispatches and awakening ids.
pub struct ContextOutcomeDrain {
    /// Outgoing dispatches to be sent.
    pub outgoing_dispatches: Vec<OutgoingMessageInfo<Dispatch>>,
    /// Messages to be waken.
    pub awakening: Vec<(MessageId, u32)>,
    /// Reply deposits to be provided.
    pub reply_deposits: Vec<(MessageId, u64)>,
    /// Whether this execution sent out a reply.
    pub reply_sent: bool,
}

/// Context outcome.
///
/// Contains all outgoing messages and wakes that should be done after execution.
#[derive(Clone, Debug)]
pub struct ContextOutcome {
    init: Vec<OutgoingMessageInfo<InitMessage>>,
    handle: Vec<OutgoingMessageInfo<HandleMessage>>,
    reply: Option<OutgoingMessageInfoNoDelay<ReplyMessage>>,
    // u32 is delay
    awakening: Vec<(MessageId, u32)>,
    // u64 is gas limit
    // TODO: add Option<ReservationId> after #1828
    reply_deposits: Vec<(MessageId, u64)>,
    // Additional information section.
    program_id: ActorId,
    source: ActorId,
    origin_msg_id: MessageId,
}

impl ContextOutcome {
    /// Create new ContextOutcome.
    fn new(program_id: ActorId, source: ActorId, origin_msg_id: MessageId) -> Self {
        Self {
            init: Vec::new(),
            handle: Vec::new(),
            reply: None,
            awakening: Vec::new(),
            reply_deposits: Vec::new(),
            program_id,
            source,
            origin_msg_id,
        }
    }

    /// Destructs outcome after execution and returns provided dispatches and awaken message ids.
    pub fn drain(self) -> ContextOutcomeDrain {
        let mut dispatches = Vec::new();
        let reply_sent = self.reply.is_some();

        for (msg, delay, reservation) in self.init.into_iter() {
            dispatches.push((msg.into_dispatch(self.program_id), delay, reservation));
        }

        for (msg, delay, reservation) in self.handle.into_iter() {
            dispatches.push((msg.into_dispatch(self.program_id), delay, reservation));
        }

        if let Some((msg, reservation)) = self.reply {
            dispatches.push((
                msg.into_dispatch(self.program_id, self.source, self.origin_msg_id),
                0,
                reservation,
            ));
        };

        ContextOutcomeDrain {
            outgoing_dispatches: dispatches,
            awakening: self.awakening,
            reply_deposits: self.reply_deposits,
            reply_sent,
        }
    }
}
/// Store of current temporary message execution context.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
pub struct OutgoingPayloads {
    handles: BTreeMap<u32, Option<Payload>>,
    reply: Option<Payload>,
    bytes_counter: u32,
}

/// Store of previous message execution context.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Decode, Encode, TypeInfo)]
#[cfg_attr(feature = "std", derive(serde::Serialize, serde::Deserialize))]
pub struct ContextStore {
    initialized: BTreeSet<ActorId>,
    reservation_nonce: ReservationNonce,
    system_reservation: Option<u64>,
    /// Used to prevent creating messages with the same ID in DB. Before this was achieved by using `outgoing.len()`
    /// but now it is moved to [OutgoingPayloads] thus we need to keep nonce here. Now to calculate nonce we simple increment `local_nonce`
    /// in each `init` call.
    local_nonce: u32,
}

impl ContextStore {
    // TODO: Remove, only used in migrations (#issue 3721)
    /// Create a new context store with the provided parameters.
    pub fn new(
        initialized: BTreeSet<ActorId>,
        reservation_nonce: ReservationNonce,
        system_reservation: Option<u64>,
        local_nonce: u32,
    ) -> Self {
        Self {
            initialized,
            reservation_nonce,
            system_reservation,
            local_nonce,
        }
    }

    /// Returns stored within message context reservation nonce.
    ///
    /// Will be non zero, if any reservations were created during
    /// previous execution of the message.
    pub(crate) fn reservation_nonce(&self) -> ReservationNonce {
        self.reservation_nonce
    }

    /// Set reservation nonce from gas reserver.
    ///
    /// Gas reserver has actual nonce state during/after execution.
    pub fn set_reservation_nonce(&mut self, gas_reserver: &GasReserver) {
        self.reservation_nonce = gas_reserver.nonce();
    }

    /// Set system reservation.
    pub fn add_system_reservation(&mut self, amount: u64) {
        let reservation = &mut self.system_reservation;
        *reservation = reservation
            .map(|reservation| reservation.saturating_add(amount))
            .or(Some(amount));
    }

    /// Get system reservation.
    pub fn system_reservation(&self) -> Option<u64> {
        self.system_reservation
    }
}

/// Context of currently processing incoming message.
#[derive(Clone, Debug)]
pub struct MessageContext {
    kind: DispatchKind,
    current: IncomingMessage,
    outcome: ContextOutcome,
    store: ContextStore,
    outgoing_payloads: OutgoingPayloads,
    settings: ContextSettings,
}

impl MessageContext {
    /// Create new message context.
    /// Returns `None` if outgoing messages bytes limit exceeded.
    pub fn new(dispatch: IncomingDispatch, program_id: ActorId, settings: ContextSettings) -> Self {
        let (kind, message, store) = dispatch.into_parts();

        Self {
            kind,
            outcome: ContextOutcome::new(program_id, message.source(), message.id()),
            current: message,
            store: store.unwrap_or_default(),
            outgoing_payloads: OutgoingPayloads::default(),
            settings,
        }
    }

    /// Getter for inner settings.
    pub fn settings(&self) -> &ContextSettings {
        &self.settings
    }

    /// Getter for inner dispatch kind
    pub fn kind(&self) -> DispatchKind {
        self.kind
    }

    fn check_reply_availability(&self) -> Result<(), ExecutionError> {
        if !matches!(self.kind, DispatchKind::Init | DispatchKind::Handle) {
            return Err(ExecutionError::IncorrectEntryForReply);
        }

        Ok(())
    }

    fn increase_counter(counter: u32, amount: impl TryInto<u32>, limit: u32) -> Option<u32> {
        TryInto::<u32>::try_into(amount)
            .ok()
            .and_then(|amount| counter.checked_add(amount))
            .and_then(|counter| (counter <= limit).then_some(counter))
    }

    /// Return bool defining was reply sent within the execution.
    pub fn reply_sent(&self) -> bool {
        self.outcome.reply.is_some()
    }

    /// Send a new program initialization message.
    ///
    /// Generates a new message from provided data packet.
    /// Returns message id and generated program id.
    pub fn init_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ActorId), Error> {
        let program_id = packet.destination();

        if self.store.initialized.contains(&program_id) {
            return Err(Error::DuplicateInit);
        }

        let last = self.store.local_nonce;

        if last >= self.settings.outgoing_limit {
            return Err(Error::OutgoingMessagesAmountLimitExceeded);
        }

        let message_id = MessageId::generate_outgoing(self.current.id(), last);
        let message = InitMessage::from_packet(message_id, packet);
        self.store.local_nonce += 1;
        self.outgoing_payloads.handles.insert(last, None);
        self.store.initialized.insert(program_id);
        self.outcome.init.push((message, delay, None));

        Ok((message_id, program_id))
    }

    /// Send a new program initialization message.
    ///
    /// Generates message from provided data packet and stored by handle payload.
    /// Returns message id.
    pub fn send_commit(
        &mut self,
        handle: u32,
        mut packet: HandlePacket,
        delay: u32,
        reservation: Option<ReservationId>,
    ) -> Result<MessageId, Error> {
        let outgoing = self
            .outgoing_payloads
            .handles
            .get_mut(&handle)
            .ok_or(Error::OutOfBounds)?;
        let data = outgoing.take().ok_or(Error::LateAccess)?;

        let do_send_commit = || {
            let Some(new_outgoing_bytes) = Self::increase_counter(
                self.outgoing_payloads.bytes_counter,
                packet.payload_len(),
                self.settings.outgoing_bytes_limit,
            ) else {
                return Err((Error::OutgoingMessagesBytesLimitExceeded, data));
            };

            packet
                .try_prepend(data)
                .map_err(|data| (Error::MaxMessageSizeExceed, data))?;

            let message_id = MessageId::generate_outgoing(self.current.id(), handle);
            let message = HandleMessage::from_packet(message_id, packet);

            self.outcome.handle.push((message, delay, reservation));

            // Increasing `outgoing_bytes_counter`, instead of decreasing it,
            // because this counter takes into account also messages,
            // that are already committed during this execution.
            // The message subsequent executions will recalculate this counter from
            // store outgoing messages (see `Self::new`),
            // so committed during this execution messages won't be taken into account
            // during next executions.
            self.outgoing_payloads.bytes_counter = new_outgoing_bytes;

            Ok(message_id)
        };

        do_send_commit().map_err(|(err, data)| {
            *outgoing = Some(data);
            err
        })
    }

    /// Provide space for storing payload for future message creation.
    ///
    /// Returns it's handle.
    pub fn send_init(&mut self) -> Result<u32, Error> {
        let last = self.store.local_nonce;
        if last < self.settings.outgoing_limit {
            self.store.local_nonce += 1;
            self.outgoing_payloads
                .handles
                .insert(last, Some(Default::default()));

            Ok(last)
        } else {
            Err(Error::OutgoingMessagesAmountLimitExceeded)
        }
    }

    /// Pushes payload into stored payload by handle.
    pub fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Error> {
        let data = match self.outgoing_payloads.handles.get_mut(&handle) {
            Some(Some(data)) => data,
            Some(None) => return Err(Error::LateAccess),
            None => return Err(Error::OutOfBounds),
        };

        let new_outgoing_bytes = Self::increase_counter(
            self.outgoing_payloads.bytes_counter,
            buffer.len(),
            self.settings.outgoing_bytes_limit,
        )
        .ok_or(Error::OutgoingMessagesBytesLimitExceeded)?;

        data.try_extend_from_slice(buffer)
            .map_err(|_| Error::MaxMessageSizeExceed)?;

        self.outgoing_payloads.bytes_counter = new_outgoing_bytes;

        Ok(())
    }

    /// Pushes the incoming buffer/payload into stored payload by handle.
    pub fn send_push_input(&mut self, handle: u32, range: CheckedRange) -> Result<(), Error> {
        let data = match self.outgoing_payloads.handles.get_mut(&handle) {
            Some(Some(data)) => data,
            Some(None) => return Err(Error::LateAccess),
            None => return Err(Error::OutOfBounds),
        };

        let bytes_amount = range.len();
        let CheckedRange {
            offset,
            excluded_end,
        } = range;

        let new_outgoing_bytes = Self::increase_counter(
            self.outgoing_payloads.bytes_counter,
            bytes_amount,
            self.settings.outgoing_bytes_limit,
        )
        .ok_or(Error::OutgoingMessagesBytesLimitExceeded)?;

        data.try_extend_from_slice(&self.current.payload().inner()[offset..excluded_end])
            .map_err(|_| Error::MaxMessageSizeExceed)?;

        self.outgoing_payloads.bytes_counter = new_outgoing_bytes;

        Ok(())
    }

    /// Check if provided `offset`/`len` are correct for the current payload
    /// limits. Result `CheckedRange` instance is accepted by
    /// `send_push_input`/`reply_push_input` and has the method `len`
    /// allowing to charge gas before the calls.
    pub fn check_input_range(&self, offset: u32, len: u32) -> Result<CheckedRange, Error> {
        let input_len = self.current.payload().inner().len();
        let offset = offset as usize;
        let len = len as usize;

        // Check `offset` is not out of bounds.
        if offset >= input_len {
            return Err(Error::OutOfBoundsInputSliceOffset);
        }

        // Check `len` for the current `offset` doesn't refer to the slice out of input bounds.
        let available_len = input_len - offset;
        if len > available_len {
            return Err(Error::OutOfBoundsInputSliceLength);
        }

        Ok(CheckedRange {
            offset,
            // guaranteed to be `<= input.len()`, because of the check upper
            excluded_end: offset.saturating_add(len),
        })
    }

    /// Send reply message.
    ///
    /// Generates reply from provided data packet and stored reply payload.
    /// Returns message id.
    pub fn reply_commit(
        &mut self,
        mut packet: ReplyPacket,
        reservation: Option<ReservationId>,
    ) -> Result<MessageId, ExtError> {
        self.check_reply_availability()?;

        if self.reply_sent() {
            return Err(Error::DuplicateReply.into());
        }

        let data = self.outgoing_payloads.reply.take().unwrap_or_default();

        if let Err(data) = packet.try_prepend(data) {
            self.outgoing_payloads.reply = Some(data);
            return Err(Error::MaxMessageSizeExceed.into());
        }

        let message_id = MessageId::generate_reply(self.current.id());
        let message = ReplyMessage::from_packet(message_id, packet);

        self.outcome.reply = Some((message, reservation));

        Ok(message_id)
    }

    /// Pushes payload into stored reply payload.
    pub fn reply_push(&mut self, buffer: &[u8]) -> Result<(), ExtError> {
        self.check_reply_availability()?;

        if self.reply_sent() {
            return Err(Error::LateAccess.into());
        }

        // NOTE: it's normal to not undone `get_or_insert_with` in case of error
        self.outgoing_payloads
            .reply
            .get_or_insert_with(Default::default)
            .try_extend_from_slice(buffer)
            .map_err(|_| Error::MaxMessageSizeExceed.into())
    }

    /// Return reply destination.
    pub fn reply_destination(&self) -> ActorId {
        self.outcome.source
    }

    /// Pushes the incoming message buffer into stored reply payload.
    pub fn reply_push_input(&mut self, range: CheckedRange) -> Result<(), ExtError> {
        self.check_reply_availability()?;

        if self.reply_sent() {
            return Err(Error::LateAccess.into());
        }

        let CheckedRange {
            offset,
            excluded_end,
        } = range;

        // NOTE: it's normal to not undone `get_or_insert_with` in case of error
        self.outgoing_payloads
            .reply
            .get_or_insert_with(Default::default)
            .try_extend_from_slice(&self.current.payload().inner()[offset..excluded_end])
            .map_err(|_| Error::MaxMessageSizeExceed.into())
    }

    /// Wake message by it's message id.
    pub fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Error> {
        if !self.outcome.awakening.iter().any(|v| v.0 == waker_id) {
            self.outcome.awakening.push((waker_id, delay));
            Ok(())
        } else {
            Err(Error::DuplicateWaking)
        }
    }

    /// Create deposit to handle future reply on message id was sent.
    pub fn reply_deposit(
        &mut self,
        message_id: MessageId,
        amount: u64,
    ) -> Result<(), MessageError> {
        if self
            .outcome
            .reply_deposits
            .iter()
            .any(|(mid, _)| mid == &message_id)
        {
            return Err(MessageError::DuplicateReplyDeposit);
        }

        if !self
            .outcome
            .handle
            .iter()
            .any(|(message, ..)| message.id() == message_id)
            && !self
                .outcome
                .init
                .iter()
                .any(|(message, ..)| message.id() == message_id)
        {
            return Err(MessageError::IncorrectMessageForReplyDeposit);
        }

        self.outcome.reply_deposits.push((message_id, amount));

        Ok(())
    }

    /// Current processing incoming message.
    pub fn current(&self) -> &IncomingMessage {
        &self.current
    }

    /// Current program's id.
    pub fn program_id(&self) -> ActorId {
        self.outcome.program_id
    }

    /// Destructs context after execution and returns provided outcome and store.
    pub fn drain(self) -> (ContextOutcome, ContextStore) {
        let Self { outcome, store, .. } = self;

        (outcome, store)
    }
}

pub struct CheckedRange {
    offset: usize,
    excluded_end: usize,
}

impl CheckedRange {
    pub fn len(&self) -> u32 {
        (self.excluded_end - self.offset) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use core::convert::TryInto;

    macro_rules! assert_ok {
        ( $x:expr $(,)? ) => {
            let is = $x;
            match is {
                Ok(_) => (),
                _ => assert!(false, "Expected Ok(_). Got {:#?}", is),
            }
        };
        ( $x:expr, $y:expr $(,)? ) => {
            assert_eq!($x, Ok($y));
        };
    }

    macro_rules! assert_err {
        ( $x:expr , $y:expr $(,)? ) => {
            assert_eq!($x, Err($y.into()));
        };
    }

    // Set of constants for clarity of a part of the test
    const INCOMING_MESSAGE_ID: u64 = 3;
    const INCOMING_MESSAGE_SOURCE: u64 = 4;

    #[test]
    fn duplicated_init() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        // first init to default ActorId.
        assert_ok!(message_context.init_program(Default::default(), 0));

        // second init to same default ActorId should get error.
        assert_err!(
            message_context.init_program(Default::default(), 0),
            Error::DuplicateInit,
        );
    }

    #[test]
    fn send_push_bytes_exceeded() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, 10),
        );

        let handle = message_context.send_init().unwrap();

        // push 5 bytes
        assert_ok!(message_context.send_push(handle, &[1, 2, 3, 4, 5]));

        // push 5 bytes
        assert_ok!(message_context.send_push(handle, &[1, 2, 3, 4, 5]));

        // push 1 byte should get error.
        assert_err!(
            message_context.send_push(handle, &[1]),
            Error::OutgoingMessagesBytesLimitExceeded,
        );
    }

    #[test]
    fn send_commit_bytes_exceeded() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, 10),
        );

        let handle = message_context.send_init().unwrap();

        // push 5 bytes
        assert_ok!(message_context.send_push(handle, &[1, 2, 3, 4, 5]));

        // commit 6 bytes should get error.
        assert_err!(
            message_context.send_commit(
                handle,
                HandlePacket::new(
                    Default::default(),
                    Payload::try_from([1, 2, 3, 4, 5, 6].to_vec()).unwrap(),
                    0
                ),
                0,
                None
            ),
            Error::OutgoingMessagesBytesLimitExceeded,
        );

        // commit 5 bytes should be ok.
        assert_ok!(message_context.send_commit(
            handle,
            HandlePacket::new(
                Default::default(),
                Payload::try_from([1, 2, 3, 4, 5].to_vec()).unwrap(),
                0,
            ),
            0,
            None,
        ));

        let messages = message_context.drain().0.drain().outgoing_dispatches;
        assert_eq!(
            messages[0].0.payload_bytes(),
            [1, 2, 3, 4, 5, 1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn send_commit_message_size_limit() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        let handle = message_context.send_init().unwrap();

        // push 1 byte
        assert_ok!(message_context.send_push(handle, &[1]));

        let payload = Payload::filled_with(2);
        assert_err!(
            message_context.send_commit(
                handle,
                HandlePacket::new(Default::default(), payload, 0),
                0,
                None
            ),
            Error::MaxMessageSizeExceed,
        );

        let payload = Payload::try_from(vec![1; Payload::max_len() - 1]).unwrap();
        assert_ok!(message_context.send_commit(
            handle,
            HandlePacket::new(Default::default(), payload, 0),
            0,
            None,
        ));

        let messages = message_context.drain().0.drain().outgoing_dispatches;
        assert_eq!(
            Payload::try_from(messages[0].0.payload_bytes().to_vec()).unwrap(),
            Payload::filled_with(1)
        );
    }

    #[test]
    fn send_push_input_bytes_exceeded() {
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ActorId::from(INCOMING_MESSAGE_SOURCE),
            vec![1, 2, 3, 4, 5].try_into().unwrap(),
            0,
            0,
            None,
        );

        let incoming_dispatch = IncomingDispatch::new(DispatchKind::Handle, incoming_message, None);

        // Creating a message context
        let mut message_context = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, 10),
        );

        let handle = message_context.send_init().unwrap();

        // push 5 bytes
        assert_ok!(message_context.send_push_input(
            handle,
            CheckedRange {
                offset: 0,
                excluded_end: 5,
            }
        ));

        // push 5 bytes
        assert_ok!(message_context.send_push_input(
            handle,
            CheckedRange {
                offset: 0,
                excluded_end: 5,
            }
        ));

        // push 1 byte should get error.
        assert_err!(
            message_context.send_push_input(
                handle,
                CheckedRange {
                    offset: 0,
                    excluded_end: 1,
                }
            ),
            Error::OutgoingMessagesBytesLimitExceeded,
        );
    }

    #[test]
    fn outgoing_limit_exceeded() {
        // Check that we can always send exactly outgoing_limit messages.
        let max_n = 5;

        for n in 0..=max_n {
            // for outgoing_limit n checking that LimitExceeded will be after n's message.
            let settings = ContextSettings::with_outgoing_limits(n, u32::MAX);

            let mut message_context =
                MessageContext::new(Default::default(), Default::default(), settings);
            // send n messages
            for _ in 0..n {
                let handle = message_context.send_init().expect("unreachable");
                message_context
                    .send_push(handle, b"payload")
                    .expect("unreachable");
                message_context
                    .send_commit(handle, HandlePacket::default(), 0, None)
                    .expect("unreachable");
            }
            // n + 1 should get first error.
            let limit_exceeded = message_context.send_init();
            assert_eq!(
                limit_exceeded,
                Err(Error::OutgoingMessagesAmountLimitExceeded)
            );

            // we can't send messages in this MessageContext.
            let limit_exceeded = message_context.init_program(Default::default(), 0);
            assert_eq!(
                limit_exceeded,
                Err(Error::OutgoingMessagesAmountLimitExceeded)
            );
        }
    }

    #[test]
    fn invalid_out_of_bounds() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        // Use invalid handle 0.
        let out_of_bounds = message_context.send_commit(0, Default::default(), 0, None);
        assert_eq!(out_of_bounds, Err(Error::OutOfBounds));

        // make 0 valid.
        let valid_handle = message_context.send_init().expect("unreachable");
        assert_eq!(valid_handle, 0);

        // Use valid handle 0.
        assert_ok!(message_context.send_commit(0, Default::default(), 0, None));

        // Use invalid handle 42.
        assert_err!(
            message_context.send_commit(42, Default::default(), 0, None),
            Error::OutOfBounds,
        );
    }

    #[test]
    fn double_reply() {
        let mut message_context = MessageContext::new(
            Default::default(),
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        // First reply.
        assert_ok!(message_context.reply_commit(Default::default(), None));

        // Reply twice in one message is forbidden.
        assert_err!(
            message_context.reply_commit(Default::default(), None),
            Error::DuplicateReply,
        );
    }

    #[test]
    fn reply_commit_message_size_limit() {
        let mut message_context =
            MessageContext::new(Default::default(), Default::default(), Default::default());

        assert_ok!(message_context.reply_push(&[1]));

        let payload = Payload::filled_with(2);
        assert_err!(
            message_context.reply_commit(ReplyPacket::new(payload, 0), None),
            Error::MaxMessageSizeExceed,
        );

        let payload = Payload::try_from(vec![1; Payload::max_len() - 1]).unwrap();
        assert_ok!(message_context.reply_commit(ReplyPacket::new(payload, 0), None));

        let messages = message_context.drain().0.drain().outgoing_dispatches;
        assert_eq!(
            Payload::try_from(messages[0].0.payload_bytes().to_vec()).unwrap(),
            Payload::filled_with(1)
        );
    }

    #[test]
    /// Test that covers full api of `MessageContext`
    fn message_context_api() {
        // Creating an incoming message around which the runner builds the `MessageContext`
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ActorId::from(INCOMING_MESSAGE_SOURCE),
            vec![1, 2].try_into().unwrap(),
            0,
            0,
            None,
        );

        let incoming_dispatch = IncomingDispatch::new(DispatchKind::Handle, incoming_message, None);

        // Creating a message context
        let mut context = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        // Checking that the initial parameters of the context match the passed constants
        assert_eq!(context.current().id(), MessageId::from(INCOMING_MESSAGE_ID));

        // Creating a reply packet
        let reply_packet = ReplyPacket::new(vec![0, 0].try_into().unwrap(), 0);

        // Checking that we are able to initialize reply
        assert_ok!(context.reply_push(&[1, 2, 3]));

        // Setting reply message and making sure the operation was successful
        assert_ok!(context.reply_commit(reply_packet.clone(), None));

        // Checking that the `ReplyMessage` matches the passed one
        assert_eq!(
            context
                .outcome
                .reply
                .as_ref()
                .unwrap()
                .0
                .payload_bytes()
                .to_vec(),
            vec![1, 2, 3, 0, 0],
        );

        // Checking that repeated call `reply_push(...)` returns error and does not do anything
        assert_err!(context.reply_push(&[1]), Error::LateAccess);
        assert_eq!(
            context
                .outcome
                .reply
                .as_ref()
                .unwrap()
                .0
                .payload_bytes()
                .to_vec(),
            vec![1, 2, 3, 0, 0],
        );

        // Checking that repeated call `reply_commit(...)` returns error and does not
        assert_err!(
            context.reply_commit(reply_packet, None),
            Error::DuplicateReply
        );

        // Checking that at this point vector of outgoing messages is empty
        assert!(context.outcome.handle.is_empty());

        // Creating an expected handle for a future initialized message
        let expected_handle = 0;

        // Initializing message and compare its handle with expected one
        assert_eq!(
            context.send_init().expect("Error initializing new message"),
            expected_handle
        );

        // And checking that it is not formed
        assert!(
            context
                .outgoing_payloads
                .handles
                .get(&expected_handle)
                .expect("This key should be")
                .is_some()
        );

        // Checking that we are able to push payload for the
        // message that we have not committed yet
        assert_ok!(context.send_push(expected_handle, &[5, 7]));
        assert_ok!(context.send_push(expected_handle, &[9]));

        // Creating an outgoing packet to commit sending by parts
        let commit_packet = HandlePacket::default();

        // Checking if commit is successful
        assert_ok!(context.send_commit(expected_handle, commit_packet, 0, None));

        // Checking that we are **NOT** able to push payload for the message or
        // commit it if we already committed it or directly pushed before
        assert_err!(
            context.send_push(expected_handle, &[5, 7]),
            Error::LateAccess,
        );
        assert_err!(
            context.send_commit(expected_handle, HandlePacket::default(), 0, None),
            Error::LateAccess,
        );

        // Creating a handle to push and do commit non-existent message
        let expected_handle = 15;

        // Checking that we also get an error when trying
        // to commit or send a non-existent message
        assert_err!(context.send_push(expected_handle, &[0]), Error::OutOfBounds);
        assert_err!(
            context.send_commit(expected_handle, HandlePacket::default(), 0, None),
            Error::OutOfBounds,
        );

        // Creating a handle to init and do not commit later
        // to show that the message will not be sent
        let expected_handle = 1;

        assert_eq!(
            context.send_init().expect("Error initializing new message"),
            expected_handle
        );
        assert_ok!(context.send_push(expected_handle, &[2, 2]));

        // Checking that reply message not lost and matches our initial
        assert!(context.outcome.reply.is_some());
        assert_eq!(
            context.outcome.reply.as_ref().unwrap().0.payload_bytes(),
            vec![1, 2, 3, 0, 0]
        );

        // Checking that on drain we get only messages that were fully formed (directly sent or committed)
        let (expected_result, _) = context.drain();
        assert_eq!(expected_result.handle.len(), 1);
        assert_eq!(expected_result.handle[0].0.payload_bytes(), vec![5, 7, 9]);
    }

    #[test]
    fn duplicate_waking() {
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ActorId::from(INCOMING_MESSAGE_SOURCE),
            vec![1, 2].try_into().unwrap(),
            0,
            0,
            None,
        );

        let incoming_dispatch = IncomingDispatch::new(DispatchKind::Handle, incoming_message, None);

        let mut context = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        context.wake(MessageId::default(), 10).unwrap();

        assert_eq!(
            context.wake(MessageId::default(), 1),
            Err(Error::DuplicateWaking)
        );
    }

    #[test]
    fn duplicate_reply_deposit() {
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ActorId::from(INCOMING_MESSAGE_SOURCE),
            vec![1, 2].try_into().unwrap(),
            0,
            0,
            None,
        );

        let incoming_dispatch = IncomingDispatch::new(DispatchKind::Handle, incoming_message, None);

        let mut message_context = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        let handle = message_context.send_init().expect("unreachable");
        message_context
            .send_push(handle, b"payload")
            .expect("unreachable");
        let message_id = message_context
            .send_commit(handle, HandlePacket::default(), 0, None)
            .expect("unreachable");

        assert!(message_context.reply_deposit(message_id, 1234).is_ok());
        assert_err!(
            message_context.reply_deposit(message_id, 1234),
            MessageError::DuplicateReplyDeposit
        );
    }

    #[test]
    fn inexistent_reply_deposit() {
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ActorId::from(INCOMING_MESSAGE_SOURCE),
            vec![1, 2].try_into().unwrap(),
            0,
            0,
            None,
        );

        let incoming_dispatch = IncomingDispatch::new(DispatchKind::Handle, incoming_message, None);

        let mut message_context = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, u32::MAX),
        );

        let message_id = message_context
            .reply_commit(ReplyPacket::default(), None)
            .expect("unreachable");

        assert_err!(
            message_context.reply_deposit(message_id, 1234),
            MessageError::IncorrectMessageForReplyDeposit
        );
        assert_err!(
            message_context.reply_deposit(Default::default(), 1234),
            MessageError::IncorrectMessageForReplyDeposit
        );
    }
}
