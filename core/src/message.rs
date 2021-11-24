// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Message processing module and context.

use alloc::{rc::Rc, vec::Vec};

use core::cell::RefCell;
use core::fmt;

use crate::program::ProgramId;
use codec::{Decode, Encode};

/// Message payload.
#[derive(Clone, Debug, Decode, Default, Encode, derive_more::From, PartialEq, Eq)]
pub struct Payload(Vec<u8>);

impl Payload {
    /// Return raw bytes of the message payload.
    pub fn into_raw(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for Payload {
    /// Raw bytes as reference.
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

/// Message identifier.
#[derive(
    Clone,
    Copy,
    Debug,
    Decode,
    Default,
    Encode,
    derive_more::From,
    Hash,
    Ord,
    PartialOrd,
    PartialEq,
    Eq,
)]
pub struct MessageId([u8; 32]);

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Ok(hex) = crate::util::encode_hex(&self.0[..]) {
            write!(f, "{}", hex)
        } else {
            Err(fmt::Error)
        }
    }
}

impl From<u64> for MessageId {
    fn from(v: u64) -> Self {
        let mut id = [0u8; 32];
        id[0..8].copy_from_slice(&v.to_le_bytes());
        Self(id)
    }
}

impl MessageId {
    /// Create new message id from bytes.
    ///
    /// Will panic if slice is not 32 bytes length.
    pub fn from_slice(s: &[u8]) -> Self {
        if s.len() != 32 {
            panic!("Slice is not 32 bytes length")
        };
        let mut id = [0u8; 32];
        id.copy_from_slice(s);
        Self(id)
    }

    /// Return reference to raw bytes of this program id.
    pub fn as_slice(&self) -> &[u8] {
        &self.0[..]
    }

    /// Return mutable reference to raw bytes of this program id.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

/// Exit code type for message replies
pub type ExitCode = i32;

/// Error using messages.
#[derive(Debug)]
pub enum Error {
    /// Message limit exceeded.
    LimitExceeded,
    /// Duplicate reply message.
    DuplicateReply,
    /// Duplicate waiting message.
    DuplicateWaiting,
    /// An attempt to commit or to push a payload into an already formed message.
    LateAccess,
    /// No message found with given handle, or handle exceedes the maximum messages amount.
    OutOfBounds,
    /// An attempt to push a payload into reply that was not set
    NoReplyFound,
    /// An attempt to interrupt execution with `wait(..)` while some messages weren't completed
    UncommitedPayloads,
}

/// The common part of all types of messages.
#[derive(Clone, Debug, Decode, Default, Encode, PartialEq, Eq)]
pub struct InnerMessage {
    /// Id of the message.
    pub id: MessageId,
    /// Message payload.
    pub payload: Vec<u8>,
    /// Gas limit for the message dispatch.
    pub gas_limit: u64,
    /// Value associated with the message.
    pub value: u128,
}

/// Incoming message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    /// Inner message.
    inner: InnerMessage,
    /// Source of the message.
    source: ProgramId,
    /// In reply of.
    reply: Option<(MessageId, ExitCode)>,
}

impl From<Message> for IncomingMessage {
    fn from(msg: Message) -> Self {
        IncomingMessage {
            inner: msg.inner,
            source: msg.source,
            reply: msg.reply,
        }
    }
}

impl IncomingMessage {
    /// Create a new incoming message from `id`, `payload`, `gas_limit`, and `value`.
    pub fn from_parts(id: MessageId, payload: Vec<u8>, gas_limit: u64, value: u128) -> Self {
        Self {
            inner: InnerMessage {
                id,
                payload,
                gas_limit,
                value,
            },
            source: Default::default(),
            reply: None,
        }
    }

    /// Set the source ID and return `Self`.
    pub fn with_source(mut self, source: ProgramId) -> Self {
        self.source = source;
        self
    }

    /// Set the reply ID and exit code and return `Self`.
    pub fn with_reply(mut self, reply: MessageId, exit_code: ExitCode) -> Self {
        self.reply = Some((reply, exit_code));
        self
    }

    /// Convert the incoming message to the stored message by providing a destination ID.
    pub fn into_message(self, dest: ProgramId) -> Message {
        Message {
            inner: self.inner,
            source: self.source,
            dest,
            reply: self.reply,
        }
    }

    /// ID of the message.
    pub fn id(&self) -> MessageId {
        self.inner.id
    }

    /// Payload assotiated with the message.
    pub fn payload(&self) -> &[u8] {
        &self.inner.payload
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.inner.value
    }

    /// Source of the incoming message.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// What this message is a reply to.
    pub fn reply(&self) -> Option<(MessageId, ExitCode)> {
        self.reply
    }
}

/// Outgoing message.
#[derive(Clone, Debug, Decode, Default, Encode)]
pub struct OutgoingMessage {
    /// Inner message.
    inner: InnerMessage,
    /// Destination of the message.
    dest: ProgramId,
}

impl OutgoingMessage {
    /// Create a new outgoing message from `id`, `payload`, `gas_limit`, and `value`.
    pub fn from_parts(id: MessageId, payload: Vec<u8>, gas_limit: u64, value: u128) -> Self {
        Self {
            inner: InnerMessage {
                id,
                payload,
                gas_limit,
                value,
            },
            dest: Default::default(),
        }
    }

    /// Set the destination ID and return `Self`.
    pub fn with_dest(mut self, dest: ProgramId) -> Self {
        self.dest = dest;
        self
    }

    /// Convert the outgoing message to the stored message by providing a source ID.
    pub fn into_message(self, source: ProgramId) -> Message {
        Message {
            inner: self.inner,
            source,
            dest: self.dest,
            reply: None,
        }
    }

    /// ID of the message.
    pub fn id(&self) -> MessageId {
        self.inner.id
    }

    /// Payload assotiated with the message.
    pub fn payload(&self) -> &[u8] {
        &self.inner.payload
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.inner.value
    }

    /// Destination of the outgoing message.
    pub fn dest(&self) -> ProgramId {
        self.dest
    }
}

/// Reply message.
#[derive(Clone, Debug, Decode, Default, Encode, PartialEq, Eq)]
pub struct ReplyMessage {
    /// Inner message.
    inner: InnerMessage,
    /// Exit code.
    exit_code: ExitCode,
}

impl ReplyMessage {
    /// Convert to generic message providing extra info.
    pub fn into_message(
        self,
        source_message: MessageId,
        source_program: ProgramId,
        dest: ProgramId,
    ) -> Message {
        Message {
            inner: self.inner,
            source: source_program,
            dest,
            reply: Some((source_message, self.exit_code)),
        }
    }

    /// ID of the message.
    pub fn id(&self) -> MessageId {
        self.inner.id
    }

    /// Payload assotiated with the message.
    pub fn payload(&self) -> &[u8] {
        &self.inner.payload
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.inner.value
    }

    /// Exit code.
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}

/// Message.
#[derive(Clone, Debug, Decode, Default, Encode, PartialEq, Eq)]
pub struct Message {
    /// Inner message.
    inner: InnerMessage,
    /// Source of the message.
    source: ProgramId,
    /// Destination of the message.
    dest: ProgramId,
    /// In reply of.
    reply: Option<(MessageId, ExitCode)>,
}

impl Message {
    /// Create a new message from `id`, `payload`, `gas_limit`, and `value`.
    pub fn from_parts(id: MessageId, payload: Vec<u8>, gas_limit: u64, value: u128) -> Self {
        Self {
            inner: InnerMessage {
                id,
                payload,
                gas_limit,
                value,
            },
            source: Default::default(),
            dest: Default::default(),
            reply: None,
        }
    }

    /// Create a new message from [`InnerMessage`].
    pub fn from_inner(inner: InnerMessage) -> Self {
        Self {
            inner,
            source: Default::default(),
            dest: Default::default(),
            reply: None,
        }
    }

    /// Set the source ID and return `Self`.
    pub fn with_source(mut self, source: ProgramId) -> Self {
        self.source = source;
        self
    }

    /// Set the destination ID and return `Self`.
    pub fn with_dest(mut self, dest: ProgramId) -> Self {
        self.dest = dest;
        self
    }

    /// Set the reply and return `Self`.
    pub fn with_reply(mut self, reply: Option<(MessageId, ExitCode)>) -> Self {
        self.reply = reply;
        self
    }

    /*/// Set the reply ID and exit code and return `Self`.
    pub fn with_reply_parts(mut self, reply: MessageId, exit_code: ExitCode) -> Self {
        self.reply = Some((reply, exit_code));
        self
    }*/

    /// ID of the message.
    pub fn id(&self) -> MessageId {
        self.inner.id
    }

    /// Payload assotiated with the message.
    pub fn payload(&self) -> &[u8] {
        &self.inner.payload
    }

    /// Drain payload assotiated with the message.
    pub fn drain_payload(&mut self) -> alloc::vec::Drain<u8> {
        self.inner.payload.drain(..)
    }

    /// Set payload assotiated with the message.
    pub fn set_payload(&mut self, payload: Vec<u8>) {
        self.inner.payload = payload;
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }

    /// Set gas limit of the message.
    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        self.inner.gas_limit = gas_limit;
    }

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.inner.value
    }

    /// Source of the message.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Destination of the message.
    pub fn dest(&self) -> ProgramId {
        self.dest
    }

    /// What this message is a reply to.
    pub fn reply(&self) -> Option<(MessageId, ExitCode)> {
        self.reply
    }
}

/// Generator of message id.
pub trait MessageIdGenerator {
    /// Generate next id.
    fn next(&mut self) -> MessageId;

    /// Query current nonce.
    fn current(&self) -> u64;
}

/// Inner packet.
#[derive(Clone, Debug, Decode, Default, Encode, PartialEq, Eq)]
pub struct InnerPacket {
    /// Packet payload.
    pub payload: Vec<u8>,
    /// Gas limit for the packet dispatch.
    pub gas_limit: u64,
    /// Value associated with the packet.
    pub value: u128,
}

impl InnerPacket {
    /// Convert `InnerPacket` to [`InnerMessage`].
    pub fn into_message(self, id: MessageId) -> InnerMessage {
        InnerMessage {
            id,
            payload: self.payload,
            gas_limit: self.gas_limit,
            value: self.value,
        }
    }
}

/// Outgoing message packet.
#[derive(Clone, Debug, Decode, Default, Encode)]
pub struct OutgoingPacket {
    /// Inner packet.
    inner: InnerPacket,
    /// Destination of the packet.
    dest: ProgramId,
}

impl OutgoingPacket {
    /// Create a new outgoing packet `payload`, `gas_limit`, and `value`.
    pub fn from_parts(payload: Vec<u8>, gas_limit: u64, value: u128) -> Self {
        Self {
            inner: InnerPacket {
                payload,
                gas_limit,
                value,
            },
            dest: Default::default(),
        }
    }

    /// Set the destination ID and return `Self`.
    pub fn with_dest(mut self, dest: ProgramId) -> Self {
        self.dest = dest;
        self
    }

    /// Convert `OutgoingMessage` to [`OutgoingMessage`] using provided message ID generator.
    pub fn into_message(self, id_generator: &mut impl MessageIdGenerator) -> OutgoingMessage {
        let id = id_generator.next();
        OutgoingMessage {
            inner: self.inner.into_message(id),
            dest: self.dest,
        }
    }

    /// Gas limit of the packet.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }
}

/// Reply message packet.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ReplyPacket {
    /// Inner packet.
    inner: InnerPacket,
    /// Exit code
    exit_code: ExitCode,
}

impl ReplyPacket {
    /// Create a new outgoing packet from `payload`, `gas_limit`, and `value`.
    pub fn from_parts(payload: Vec<u8>, gas_limit: u64, value: u128) -> Self {
        Self {
            inner: InnerPacket {
                payload,
                gas_limit,
                value,
            },
            exit_code: 0,
        }
    }

    /// Set the exit code ID and return `Self`.
    pub fn with_exit_code(mut self, exit_code: ExitCode) -> Self {
        self.exit_code = exit_code;
        self
    }

    /// Convert `ReplyPacket` to [`ReplyMessage`] using provided message ID generator.
    pub fn into_message(self, id_generator: &mut impl MessageIdGenerator) -> ReplyMessage {
        let id = id_generator.next();
        ReplyMessage {
            inner: self.inner.into_message(id),
            exit_code: self.exit_code,
        }
    }

    /// Gas limit of the packet.
    pub fn gas_limit(&self) -> u64 {
        self.inner.gas_limit
    }
}

/// Message state of the current session.
///
/// Contains all generated outgoing messages with their formation statuses.
#[derive(Debug, Default)]
pub struct MessageState {
    /// Collection of outgoing messages generated.
    pub outgoing: Vec<OutgoingMessage>,
    /// Reply generated.
    pub reply: Option<ReplyMessage>,
    /// Messages to be waken.
    pub awakening: Vec<(MessageId, u64)>,
}

/// Message context for the currently running program.
#[derive(Clone)]
pub struct MessageContext<IG: MessageIdGenerator + 'static> {
    state: Rc<RefCell<MessageState>>,
    outgoing_payloads: Vec<Option<Payload>>,
    outgoing_limit: usize,
    reply_payload: Option<Payload>,
    current: Rc<IncomingMessage>,
    id_generator: Rc<RefCell<IG>>,
}

impl<IG: MessageIdGenerator + 'static> MessageContext<IG> {
    /// New context.
    ///
    /// Create context by providing incoming message for the program.
    pub fn new(incoming_message: IncomingMessage, id_generator: IG) -> MessageContext<IG> {
        MessageContext {
            state: Default::default(),
            outgoing_payloads: Vec::new(),
            outgoing_limit: 128,
            reply_payload: None,
            current: Rc::new(incoming_message),
            id_generator: Rc::new(id_generator.into()),
        }
    }

    /// Mark message as fully formed and ready for sending in this context by handle.
    pub fn send_commit(
        &mut self,
        handle: usize,
        packet: OutgoingPacket,
    ) -> Result<MessageId, Error> {
        if handle >= self.outgoing_payloads.len() {
            return Err(Error::OutOfBounds);
        }

        match self.outgoing_payloads[handle].take() {
            Some(payload) => {
                let id_generator = &mut *self.id_generator.borrow_mut();
                let mut outgoing = packet.into_message(id_generator);
                outgoing.inner.payload.splice(0..0, payload.0);

                let id = outgoing.inner.id;
                let mut state = self.state.borrow_mut();
                state.outgoing.push(outgoing);
                Ok(id)
            }
            None => Err(Error::LateAccess),
        }
    }

    /// Initialize a new message with `NotFormed` formation status and return its handle.
    ///
    /// Messages created this way should be commited with `commit(handle)` to be sent.
    pub fn send_init(&mut self) -> Result<usize, Error> {
        let state = self.state.borrow();

        // TODO: Decide whether we should limit formed messages vs. uncompleted
        if state.outgoing.len() >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        let handle = self.outgoing_payloads.len();
        self.outgoing_payloads.push(Some(Payload::default()));

        Ok(handle)
    }

    /// Push an extra buffer into message payload by handle.
    pub fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Error> {
        if handle >= self.outgoing_payloads.len() {
            return Err(Error::OutOfBounds);
        }

        if let Some(Some(payload)) = self.outgoing_payloads.get_mut(handle) {
            payload.0.extend_from_slice(buffer);
            Ok(())
        } else {
            Err(Error::LateAccess)
        }
    }

    /// Record reply to the current message.
    pub fn reply_commit(&mut self, packet: ReplyPacket) -> Result<MessageId, Error> {
        let mut state = self.state.borrow_mut();
        match &mut state.reply {
            Some(_) => Err(Error::LateAccess),
            None => {
                let id_generator = &mut *self.id_generator.borrow_mut();
                let mut reply = packet.into_message(id_generator);

                reply
                    .inner
                    .payload
                    .splice(0..0, self.reply_payload.take().unwrap_or_default().0);

                let id = reply.inner.id;
                state.reply = Some(reply);
                Ok(id)
            }
        }
    }

    /// Push an extra buffer into reply message.
    pub fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Error> {
        let state = self.state.borrow();
        if state.reply.is_some() {
            return Err(Error::LateAccess);
        }

        match &mut self.reply_payload {
            Some(payload) => payload.0.extend_from_slice(buffer),
            None => self.reply_payload = Some(buffer.to_vec().into()),
        }

        Ok(())
    }

    /// Check whether there are uncommited messages.
    pub fn check_uncommited(&self) -> Result<(), Error> {
        if self.reply_payload.is_some() {
            return Err(Error::UncommitedPayloads);
        }

        for outgoing_payload in self.outgoing_payloads.iter() {
            if outgoing_payload.is_some() {
                return Err(Error::UncommitedPayloads);
            }
        }
        Ok(())
    }

    /// Mark a message to be woken using `waker_id`.
    pub fn wake(&self, waker_id: MessageId, gas_limit: u64) -> Result<(), Error> {
        self.state
            .borrow_mut()
            .awakening
            .push((waker_id, gas_limit));
        Ok(())
    }

    /// Return reference to the current incoming message.
    pub fn current(&self) -> &IncomingMessage {
        self.current.as_ref()
    }

    /// Last used nonce
    pub fn nonce(&self) -> u64 {
        self.id_generator.borrow().current()
    }

    /// Convert this context into the message state.
    ///
    /// Do it to return all outgoing, reply, waiting, ane awakening messages generated using this context.
    pub fn into_state(self) -> MessageState {
        let Self { state, .. } = self;
        Rc::try_unwrap(state)
            .expect("Calling drain with references to the memory context left")
            .into_inner()
    }
}

#[cfg(test)]
/// This module contains tests of the `MessageContext` structure
/// functionality from the `message.rs` module
mod tests {
    use super::*;
    use alloc::vec;

    // Struct that would produce MessageId generation
    pub struct BlakeMessageIdGenerator {
        program_id: ProgramId,
        nonce: u64,
    }

    impl MessageIdGenerator for BlakeMessageIdGenerator {
        fn next(&mut self) -> MessageId {
            let mut data: Vec<u8> = self.program_id.as_slice().to_vec();
            data.push(self.nonce as u8);
            data.remove(0);

            self.nonce += 1;

            MessageId::from_slice(&data)
        }

        fn current(&self) -> u64 {
            self.nonce
        }
    }

    // Set of constants for clarity of a part of the test
    const DEFAULT_GENERATOR_PROGRAM_ID: u64 = 1;
    const DEFAULT_NONCE: u64 = 2;
    const INCOMING_MESSAGE_ID: u64 = 3;
    const INCOMING_MESSAGE_SOURCE: u64 = 4;
    const OUTGOING_MESSAGE_DEST: u64 = 5;

    #[test]
    /// Test that covers full api of `MessageContext`
    fn message_context_api() {
        // Creating an id generator
        let id_generator = BlakeMessageIdGenerator {
            program_id: ProgramId::from(DEFAULT_GENERATOR_PROGRAM_ID),
            nonce: DEFAULT_NONCE,
        };
        // Creating an incoming message around which the runner builds the `MessageContext`
        let incoming_message =
            IncomingMessage::from_parts(INCOMING_MESSAGE_ID.into(), vec![1, 2], 0, 0)
                .with_source(INCOMING_MESSAGE_SOURCE.into());

        // Creating a message context
        let mut context = MessageContext::new(incoming_message, id_generator);

        // Сhecking that the initial parameters of the context match the passed constants
        assert_eq!(context.current().id(), MessageId::from(INCOMING_MESSAGE_ID));
        assert_eq!(context.nonce(), DEFAULT_NONCE);
        assert!(context.reply_payload.is_none());
        assert!(context.state.borrow().reply.is_none());

        // Creating a reply packet
        let reply_packet = ReplyPacket::from_parts(vec![0, 0], 0, 0);

        // Checking that we are able to initialize reply
        assert!(context.reply_push(&[1, 2, 3]).is_ok());

        // Setting reply message and making sure the operation was successful
        assert!(context.reply_commit(reply_packet.clone()).is_ok());

        // After every successful generation of `Message`, `nonse` increases by one
        assert_eq!(context.nonce(), DEFAULT_NONCE + 1);

        // Checking that the `ReplyMessage` mathes the passed one
        assert_eq!(
            context.state.borrow().reply.as_ref().unwrap().payload(),
            vec![1, 2, 3, 0, 0]
        );

        // Checking that repeated call `reply_push(...)` returns error and does not do anything
        assert!(context.reply_push(&[1]).is_err());
        assert_eq!(
            context.state.borrow().reply.as_ref().unwrap().payload(),
            vec![1, 2, 3, 0, 0]
        );

        // Checking that repeated call `reply_commit(...)` returns error and does not
        // increase nonse, because `ReplyMessage` is not generated
        assert!(context.reply_commit(reply_packet.clone()).is_err());
        assert_eq!(context.nonce(), DEFAULT_NONCE + 1);

        // Checking that at this point vector of outgoing messages is empty
        assert!(context.state.borrow_mut().outgoing.is_empty());

        // Creating an expected handle for a future initialized message
        let expected_handle = 0;

        // Initializing message and compare its handle with expected one
        assert_eq!(
            context.send_init().expect("Error initializing new message"),
            expected_handle
        );

        // And checking that it is not formed
        assert!(context.outgoing_payloads[expected_handle].is_some());

        // Checking that we are able to push payload for the
        // message that we have not commited yet
        assert!(context.send_push(expected_handle, &[5, 7]).is_ok());
        assert!(context.send_push(expected_handle, &[9]).is_ok());

        // Creating an outgoing packet to commit sending by parts
        let commit_packet =
            OutgoingPacket::default().with_dest(ProgramId::from(OUTGOING_MESSAGE_DEST + 1));

        // Checking if commit is successful
        assert!(context.send_commit(expected_handle, commit_packet).is_ok());

        // Checking that we are **NOT** able to push payload for the message or
        // commit it if we already commited it or directly pushed before
        assert!(context.send_push(0, &[5, 7]).is_err());
        assert!(context.send_push(expected_handle, &[5, 7]).is_err());
        assert!(context.send_commit(0, OutgoingPacket::default()).is_err());
        assert!(context
            .send_commit(expected_handle, OutgoingPacket::default())
            .is_err());

        // Checking that we also get an error when trying
        // to commit or send a non-existent message
        assert!(context.send_push(15, &[0]).is_err());
        assert!(context.send_commit(15, OutgoingPacket::default()).is_err());

        // Creating a handle to init and do not commit later
        // to show that the message will not be sent
        let expected_handle = 1;

        assert_eq!(
            context.send_init().expect("Error initializing new message"),
            expected_handle
        );
        assert!(context.send_push(expected_handle, &[2, 2]).is_ok());

        // Checking that reply message not lost and matches our initial
        assert!(context.state.borrow().reply.is_some());
        assert_eq!(
            context.state.borrow().reply.as_ref().unwrap().payload(),
            vec![1, 2, 3, 0, 0]
        );

        // Checking that on drain we get only messages that were fully formed (directly sent or commited)
        let expected_result = context.into_state();
        assert_eq!(expected_result.outgoing.len(), 1);
        assert_eq!(expected_result.outgoing[0].payload(), vec![5, 7, 9]);
    }
}
