// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
    ids::{MessageId, ProgramId, ReservationId},
    message::{
        Dispatch, HandleMessage, HandlePacket, IncomingMessage, InitMessage, InitPacket, Payload,
        ReplyMessage, ReplyPacket,
    },
    reservation::{GasReserver, ReservationNonce},
};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};
use gear_core_errors::{ExecutionError, ExtError, MessageError as Error, MessageError};
use scale_info::{
    scale::{Decode, Encode},
    TypeInfo,
};

use super::{DispatchKind, IncomingDispatch, Packet};

/// Context settings.
#[derive(Copy, Clone, Debug, Default)]
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
#[derive(Default, Debug)]
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
    program_id: ProgramId,
    source: ProgramId,
    origin_msg_id: MessageId,
}

impl ContextOutcome {
    /// Create new ContextOutcome.
    fn new(program_id: ProgramId, source: ProgramId, origin_msg_id: MessageId) -> Self {
        Self {
            program_id,
            source,
            origin_msg_id,
            ..Default::default()
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

/// Store of previous message execution context.
#[derive(Clone, Default, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Decode, Encode, TypeInfo)]
pub struct ContextStore {
    outgoing: BTreeMap<u32, Option<Payload>>,
    reply: Option<Payload>,
    initialized: BTreeSet<ProgramId>,
    reservation_nonce: ReservationNonce,
    system_reservation: Option<u64>,
}

impl ContextStore {
    // TODO: Remove, only used in migrations (#issue 3721)
    /// Create a new context store with the provided parameters.
    pub fn new(
        outgoing: BTreeMap<u32, Option<Payload>>,
        reply: Option<Payload>,
        initialized: BTreeSet<ProgramId>,
        reservation_nonce: ReservationNonce,
        system_reservation: Option<u64>,
    ) -> Self {
        Self {
            outgoing,
            reply,
            initialized,
            reservation_nonce,
            system_reservation,
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
#[derive(Debug)]
pub struct MessageContext {
    kind: DispatchKind,
    current: IncomingMessage,
    outcome: ContextOutcome,
    store: ContextStore,
    settings: ContextSettings,
    outgoing_bytes_counter: u32,
}

impl MessageContext {
    /// Create new message context.
    /// Returns `None` if outgoing messages bytes limit exceeded.
    pub fn new(
        dispatch: IncomingDispatch,
        program_id: ProgramId,
        settings: ContextSettings,
    ) -> Option<Self> {
        let (kind, message, store) = dispatch.into_parts();

        let outgoing_bytes_counter = match &store {
            Some(store) => {
                let mut counter = 0u32;
                for payload in store.outgoing.values().filter_map(|x| x.as_ref()) {
                    counter = counter.checked_add(payload.len_u32())?;
                }
                counter
            }
            None => 0,
        };

        if outgoing_bytes_counter > settings.outgoing_bytes_limit {
            // Outgoing messages bytes limit exceeded.
            return None;
        }

        Some(Self {
            kind,
            outcome: ContextOutcome::new(program_id, message.source(), message.id()),
            current: message,
            store: store.unwrap_or_default(),
            settings,
            outgoing_bytes_counter,
        })
    }

    /// Getter for inner settings.
    pub fn settings(&self) -> &ContextSettings {
        &self.settings
    }

    fn check_reply_availability(&self) -> Result<(), ExecutionError> {
        if !matches!(self.kind, DispatchKind::Init | DispatchKind::Handle) {
            return Err(ExecutionError::IncorrectEntryForReply);
        }

        Ok(())
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
    ) -> Result<(MessageId, ProgramId), Error> {
        let program_id = packet.destination();

        if self.store.initialized.contains(&program_id) {
            return Err(Error::DuplicateInit);
        }

        let last = self.store.outgoing.len() as u32;

        if last >= self.settings.outgoing_limit {
            return Err(Error::OutgoingMessagesAmountLimitExceeded);
        }

        let message_id = MessageId::generate_outgoing(self.current.id(), last);
        let message = InitMessage::from_packet(message_id, packet);

        self.store.outgoing.insert(last, None);
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
        packet: HandlePacket,
        delay: u32,
        reservation: Option<ReservationId>,
    ) -> Result<MessageId, Error> {
        if let Some(payload) = self.store.outgoing.get_mut(&handle) {
            if let Some(data) = payload.take() {
                let Some(new_outgoing_bytes) = self
                    .outgoing_bytes_counter
                    .checked_add(packet.payload_len())
                    .and_then(|counter| {
                        (counter <= self.settings.outgoing_bytes_limit).then_some(counter)
                    })
                else {
                    *payload = Some(data);
                    return Err(Error::OutgoingMessagesBytesLimitExceeded);
                };

                // TODO: set data back if error #3779
                let packet = {
                    let mut packet = packet;
                    packet
                        .try_prepend(data)
                        .map_err(|_| Error::MaxMessageSizeExceed)?;
                    packet
                };

                let message_id = MessageId::generate_outgoing(self.current.id(), handle);
                let message = HandleMessage::from_packet(message_id, packet);

                self.outcome.handle.push((message, delay, reservation));

                // Increasing `outgoing_bytes_counter`, instead of decreasing it, because
                // this counter takes into account also messages, that are already committed
                // during this execution.
                // The message subsequent executions will recalculate this counter from
                // store outgoing messages (see `Self::new`),
                // so committed during this execution messages won't be taken into account
                // during next executions.
                self.outgoing_bytes_counter = new_outgoing_bytes;

                Ok(message_id)
            } else {
                Err(Error::LateAccess)
            }
        } else {
            Err(Error::OutOfBounds)
        }
    }

    /// Provide space for storing payload for future message creation.
    ///
    /// Returns it's handle.
    pub fn send_init(&mut self) -> Result<u32, Error> {
        let last = self.store.outgoing.len() as u32;

        if last < self.settings.outgoing_limit {
            self.store.outgoing.insert(last, Some(Default::default()));

            Ok(last)
        } else {
            Err(Error::OutgoingMessagesAmountLimitExceeded)
        }
    }

    /// Pushes payload into stored payload by handle.
    pub fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Error> {
        match self.store.outgoing.get_mut(&handle) {
            Some(Some(data)) => {
                let new_outgoing_bytes = u32::try_from(buffer.len())
                    .ok()
                    .and_then(|bytes_amount| self.outgoing_bytes_counter.checked_add(bytes_amount))
                    .and_then(|counter| {
                        (counter <= self.settings.outgoing_bytes_limit).then_some(counter)
                    })
                    .ok_or(Error::OutgoingMessagesBytesLimitExceeded)?;

                data.try_extend_from_slice(buffer)
                    .map_err(|_| Error::MaxMessageSizeExceed)?;
                self.outgoing_bytes_counter = new_outgoing_bytes;
                Ok(())
            }
            Some(None) => Err(Error::LateAccess),
            None => Err(Error::OutOfBounds),
        }
    }

    /// Pushes the incoming buffer/payload into stored payload by handle.
    pub fn send_push_input(&mut self, handle: u32, range: CheckedRange) -> Result<(), Error> {
        let data = self
            .store
            .outgoing
            .get_mut(&handle)
            .ok_or(Error::OutOfBounds)?
            .as_mut()
            .ok_or(Error::LateAccess)?;

        let CheckedRange {
            offset,
            excluded_end,
        } = range;

        let bytes_amount = excluded_end.checked_sub(offset).unwrap_or_else(|| {
            unreachable!("`CheckedRange` must guarantee that `excluded_end` >= `offset`")
        });

        let new_outgoing_bytes = u32::try_from(bytes_amount)
            .ok()
            .and_then(|bytes_amount| self.outgoing_bytes_counter.checked_add(bytes_amount))
            .and_then(|counter| (counter <= self.settings.outgoing_bytes_limit).then_some(counter))
            .ok_or(Error::OutgoingMessagesBytesLimitExceeded)?;

        data.try_extend_from_slice(&self.current.payload_bytes()[offset..excluded_end])
            .map_err(|_| Error::MaxMessageSizeExceed)?;
        self.outgoing_bytes_counter = new_outgoing_bytes;

        Ok(())
    }

    /// Check if provided `offset`/`len` are correct for the current payload
    /// limits. Result `CheckedRange` instance is accepted by
    /// `send_push_input`/`reply_push_input` and has the method `len`
    /// allowing to charge gas before the calls.
    pub fn check_input_range(&self, offset: u32, len: u32) -> CheckedRange {
        let input = self.current.payload_bytes();
        let offset = offset as usize;
        if offset >= input.len() {
            return CheckedRange {
                offset: 0,
                excluded_end: 0,
            };
        }

        CheckedRange {
            offset,
            excluded_end: if len == 0 {
                offset
            } else {
                offset.saturating_add(len as usize).min(input.len())
            },
        }
    }

    /// Send reply message.
    ///
    /// Generates reply from provided data packet and stored reply payload.
    /// Returns message id.
    pub fn reply_commit(
        &mut self,
        packet: ReplyPacket,
        reservation: Option<ReservationId>,
    ) -> Result<MessageId, ExtError> {
        self.check_reply_availability()?;

        if !self.reply_sent() {
            let data = self.store.reply.take().unwrap_or_default();

            // TODO: set data back if error #3779
            let packet = {
                let mut packet = packet;
                packet
                    .try_prepend(data)
                    .map_err(|_| Error::MaxMessageSizeExceed)?;
                packet
            };

            let message_id = MessageId::generate_reply(self.current.id());
            let message = ReplyMessage::from_packet(message_id, packet);

            self.outcome.reply = Some((message, reservation));

            Ok(message_id)
        } else {
            Err(Error::DuplicateReply.into())
        }
    }

    /// Pushes payload into stored reply payload.
    pub fn reply_push(&mut self, buffer: &[u8]) -> Result<(), ExtError> {
        self.check_reply_availability()?;

        if !self.reply_sent() {
            let data = self.store.reply.get_or_insert_with(Default::default);
            data.try_extend_from_slice(buffer)
                .map_err(|_| Error::MaxMessageSizeExceed)?;

            Ok(())
        } else {
            Err(Error::LateAccess.into())
        }
    }

    /// Return reply destination.
    pub fn reply_destination(&self) -> ProgramId {
        self.outcome.source
    }

    /// Pushes the incoming message buffer into stored reply payload.
    pub fn reply_push_input(&mut self, range: CheckedRange) -> Result<(), ExtError> {
        self.check_reply_availability()?;

        if !self.reply_sent() {
            let CheckedRange {
                offset,
                excluded_end,
            } = range;

            let data = self.store.reply.get_or_insert_with(Default::default);
            data.try_extend_from_slice(&self.current.payload_bytes()[offset..excluded_end])
                .map_err(|_| Error::MaxMessageSizeExceed)?;

            Ok(())
        } else {
            Err(Error::LateAccess.into())
        }
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

    /// Mutable reference to currently processed incoming message.
    pub fn payload_mut(&mut self) -> &mut Payload {
        self.current.payload_mut()
    }

    /// Current program's id.
    pub fn program_id(&self) -> ProgramId {
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
        )
        .expect("Outgoing messages bytes limit exceeded");

        // first init to default ProgramId.
        assert_ok!(message_context.init_program(Default::default(), 0));

        // second init to same default ProgramId should get error.
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
    }

    #[test]
    fn send_push_input_bytes_exceeded() {
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ProgramId::from(INCOMING_MESSAGE_SOURCE),
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
    fn create_wrong_context() {
        let context_store = ContextStore {
            outgoing: [(1, Some(vec![1, 2].try_into().unwrap()))]
                .iter()
                .cloned()
                .collect(),
            reply: None,
            initialized: BTreeSet::new(),
            reservation_nonce: ReservationNonce::default(),
            system_reservation: None,
        };

        let incoming_dispatch = IncomingDispatch::new(
            DispatchKind::Handle,
            Default::default(),
            Some(context_store),
        );

        let ctx = MessageContext::new(
            incoming_dispatch,
            Default::default(),
            ContextSettings::with_outgoing_limits(1024, 1),
        );

        // Creating a message context must return None,
        // because of the outgoing messages bytes limit exceeded.
        assert!(ctx.is_none(), "Expect None, got {:?}", ctx);
    }

    #[test]
    fn outgoing_limit_exceeded() {
        // Check that we can always send exactly outgoing_limit messages.
        let max_n = 5;

        for n in 0..=max_n {
            // for outgoing_limit n checking that LimitExceeded will be after n's message.
            let settings = ContextSettings::with_outgoing_limits(n, u32::MAX);

            let mut message_context =
                MessageContext::new(Default::default(), Default::default(), settings)
                    .expect("Outgoing messages bytes limit exceeded");
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
        )
        .expect("Outgoing messages bytes limit exceeded");

        // First reply.
        assert_ok!(message_context.reply_commit(Default::default(), None));

        // Reply twice in one message is forbidden.
        assert_err!(
            message_context.reply_commit(Default::default(), None),
            Error::DuplicateReply,
        );
    }

    #[test]
    /// Test that covers full api of `MessageContext`
    fn message_context_api() {
        // Creating an incoming message around which the runner builds the `MessageContext`
        let incoming_message = IncomingMessage::new(
            MessageId::from(INCOMING_MESSAGE_ID),
            ProgramId::from(INCOMING_MESSAGE_SOURCE),
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
        )
        .expect("Outgoing messages bytes limit exceeded");

        // Checking that the initial parameters of the context match the passed constants
        assert_eq!(context.current().id(), MessageId::from(INCOMING_MESSAGE_ID));
        assert!(context.store.reply.is_none());
        assert!(context.outcome.reply.is_none());

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
        assert!(context
            .store
            .outgoing
            .get(&expected_handle)
            .expect("This key should be")
            .is_some());

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
            ProgramId::from(INCOMING_MESSAGE_SOURCE),
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
            ProgramId::from(INCOMING_MESSAGE_SOURCE),
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
            ProgramId::from(INCOMING_MESSAGE_SOURCE),
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
        )
        .expect("Outgoing messages bytes limit exceeded");

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
