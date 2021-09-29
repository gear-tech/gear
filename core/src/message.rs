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

impl core::convert::AsRef<[u8]> for Payload {
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
        let mut id = Self([0u8; 32]);
        id.0[0..8].copy_from_slice(&v.to_le_bytes()[..]);
        id
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
        let mut id = Self([0u8; 32]);
        id.0[..].copy_from_slice(s);
        id
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
    /// Duplicate message to be woken.
    DuplicateAwakening,
    /// An attempt to commit or to push a payload into an already formed message.
    LateAccess,
    /// No message found with given handle, or handle exceedes the maximum messages amount.
    OutOfBounds,
    /// An attempt to push a payload into reply that was not set
    NoReplyFound,
}

/// Incoming message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    id: MessageId,
    source: ProgramId,
    payload: Payload,
    gas_limit: u64,
    value: u128,
    reply: Option<(MessageId, ExitCode)>,
}

impl IncomingMessage {
    /// Source of the incoming message, if any.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Payload of the incoming message.
    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    /// Set gas limit of the message.
    pub fn set_gas_limit(&mut self, gas_limit: u64) {
        self.gas_limit = gas_limit;
    }

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.value
    }

    /// Id of the message.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// What this message is a reply to
    pub fn reply(&self) -> Option<(MessageId, ExitCode)> {
        self.reply
    }
}

impl From<Message> for IncomingMessage {
    fn from(s: Message) -> Self {
        IncomingMessage {
            id: s.id(),
            source: s.source(),
            payload: s.payload,
            gas_limit: s.gas_limit,
            value: s.value,
            reply: s.reply,
        }
    }
}

impl IncomingMessage {
    /// New incoming message from specific `source`, `payload` and `gas_limit`.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
    ) -> Self {
        Self {
            id,
            source,
            payload,
            gas_limit,
            value,
            reply: None,
        }
    }

    /// New reply message from specific `source`, `payload` and `gas_limit` and `reply`.
    pub fn new_reply(
        id: MessageId,
        source: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
        reply: MessageId,
        exit_code: ExitCode,
    ) -> Self {
        Self {
            id,
            source,
            payload,
            gas_limit,
            value,
            reply: Some((reply, exit_code)),
        }
    }

    /// New system incoming message.
    pub fn new_system(id: MessageId, payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            id,
            source: ProgramId::system(),
            payload,
            gas_limit,
            value,
            reply: None,
        }
    }

    /// Convert incoming message to the stored message by providing `dest`.
    pub fn into_message(self, dest: ProgramId) -> Message {
        Message {
            id: self.id,
            source: self.source,
            dest,
            payload: self.payload,
            gas_limit: self.gas_limit,
            value: self.value,
            reply: self.reply,
        }
    }
}

/// Outgoing message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingMessage {
    id: MessageId,
    dest: ProgramId,
    payload: Payload,
    gas_limit: u64,
    value: u128,
}

impl OutgoingMessage {
    /// New outgoing message.
    pub fn new(
        id: MessageId,
        dest: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
    ) -> Self {
        Self {
            id,
            dest,
            payload,
            gas_limit,
            value,
        }
    }

    /// Convert outgoing message to the stored message by providing `source`.
    pub fn into_message(self, source: ProgramId) -> Message {
        Message {
            id: self.id,
            source,
            dest: self.dest,
            payload: self.payload,
            gas_limit: self.gas_limit,
            value: self.value,
            reply: None,
        }
    }

    /// Return declared gas_limit of the message.
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    /// Return message id generated for this packet.
    pub fn id(&self) -> MessageId {
        self.id
    }
}

/// Reply message.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ReplyMessage {
    /// Identifier of the reply message.
    id: MessageId,
    /// Exit code
    exit_code: ExitCode,
    /// Payload of the reply message.
    payload: Payload,
    /// Gas limit.
    gas_limit: u64,
    /// Message value.
    value: u128,
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
            id: self.id,
            source: source_program,
            dest,
            payload: self.payload,
            gas_limit: self.gas_limit,
            value: self.value,
            reply: Some((source_message, self.exit_code)),
        }
    }

    /// Return message id generated for this packet.
    pub fn id(&self) -> MessageId {
        self.id
    }
}

/// Message.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Message {
    /// Id of the message
    pub id: MessageId,
    /// Source of the message.
    pub source: ProgramId,
    /// Destination of the message.
    pub dest: ProgramId,
    /// Payload of the message.
    pub payload: Payload,
    /// Gas limit.
    pub gas_limit: u64,
    /// Message value.
    pub value: u128,
    /// In reply of.
    pub reply: Option<(MessageId, ExitCode)>,
}

impl Message {
    /// New system message to the specific program.
    pub fn new_system(
        id: MessageId,
        dest: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
    ) -> Message {
        Message {
            id,
            source: 0.into(),
            dest,
            payload,
            gas_limit,
            value,
            reply: None,
        }
    }

    /// New system message to the specific program.
    pub fn new(
        id: MessageId,
        source: ProgramId,
        dest: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
    ) -> Message {
        Message {
            id,
            source,
            dest,
            payload,
            gas_limit,
            value,
            reply: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// New system message to the specific program.
    pub fn new_reply(
        id: MessageId,
        source: ProgramId,
        dest: ProgramId,
        payload: Payload,
        gas_limit: u64,
        value: u128,
        reply: MessageId,
        exit_code: ExitCode,
    ) -> Message {
        Message {
            id,
            source,
            dest,
            payload,
            gas_limit,
            value,
            reply: Some((reply, exit_code)),
        }
    }

    /// Return destination of this message.
    pub fn dest(&self) -> ProgramId {
        self.dest
    }

    /// Return source of this message.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Get the payload reference of this message.
    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }

    /// Message gas limit.
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    /// Message value.
    pub fn value(&self) -> u128 {
        self.value
    }

    /// Is message a reply and to what.
    pub fn reply(&self) -> Option<(MessageId, ExitCode)> {
        self.reply
    }

    /// Message idetifier.
    pub fn id(&self) -> MessageId {
        self.id
    }
}

/// Outgoing message packet.
#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingPacket {
    dest: ProgramId,
    payload: Payload,
    gas_limit: u64,
    value: u128,
}

impl OutgoingPacket {
    /// New outgoing message packet.
    pub fn new(dest: ProgramId, payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            dest,
            payload,
            gas_limit,
            value,
        }
    }

    /// Gas limit.
    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    /// Value.
    pub fn value(&self) -> u128 {
        self.value
    }

    /// Payload.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Destination.
    pub fn dest(&self) -> ProgramId {
        self.dest
    }
}

impl Default for OutgoingPacket {
    /// Empty packet with log dest.
    fn default() -> Self {
        Self {
            dest: ProgramId::system(),
            payload: Payload::default(),
            gas_limit: 0,
            value: 0,
        }
    }
}

/// Reply message packet.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct ReplyPacket {
    /// Payload of the reply message.
    pub payload: Payload,
    /// Gas limit.
    pub gas_limit: u64,
    /// Message value.
    pub value: u128,
    /// Exit code
    pub exit_code: ExitCode,
}

impl ReplyPacket {
    /// New reply message in some message context.
    pub fn new(exit_code: ExitCode, payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            payload,
            gas_limit,
            value,
            exit_code,
        }
    }
}

/// Generator of message id.
pub trait MessageIdGenerator {
    /// Generate next id.
    fn next(&mut self) -> MessageId;

    /// Query current nonce.
    fn current(&self) -> u64;

    /// Build outgoing message from current packet.
    ///
    /// Message id will be generated.
    fn produce_outgoing(&mut self, packet: OutgoingPacket) -> OutgoingMessage {
        let id = self.next();
        OutgoingMessage {
            id,
            dest: packet.dest,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
        }
    }

    /// Build reply from reply packet.
    ///
    /// Message id will be generated.
    fn produce_reply(&mut self, packet: ReplyPacket) -> ReplyMessage {
        let id = self.next();

        ReplyMessage {
            id,
            payload: packet.payload,
            gas_limit: packet.gas_limit,
            value: packet.value,
            exit_code: packet.exit_code,
        }
    }
}

/// Message state of the current session.
///
/// Contains all generated outgoing messages with their formation statuses.
#[derive(Debug, Default)]
pub struct MessageState {
    /// Collection of outgoing messages generated.
    pub outgoing: Vec<(Option<Payload>, Option<OutgoingMessage>)>,
    /// Reply generated.
    pub reply: (Option<Payload>, Option<ReplyMessage>),
    /// Message to be added to wait list.
    pub waiting: Option<IncomingMessage>,
    /// Message to be waken.
    pub awakening: Option<MessageId>,
}

/// Message context for the currently running program.
#[derive(Clone)]
pub struct MessageContext<IG: MessageIdGenerator + 'static> {
    state: Rc<RefCell<MessageState>>,
    outgoing_limit: usize,
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
            outgoing_limit: 128,
            current: Rc::new(incoming_message),
            id_generator: Rc::new(id_generator.into()),
        }
    }

    /// Initialize a new message with `NotFormed` formation status and return its handle.
    ///
    /// Messages created this way should be commited with `commit(handle)` to be sent.
    pub fn send_init(&self) -> Result<usize, Error> {
        let mut state = self.state.borrow_mut();

        let outgoing_count = state.outgoing.len();

        if outgoing_count >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        state.outgoing.push((Some(Payload::default()), None));

        Ok(outgoing_count)
    }

    /// Push an extra buffer into message payload by handle.
    pub fn send_push(&self, handle: usize, buffer: &[u8]) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();

        if handle >= state.outgoing.len() {
            return Err(Error::OutOfBounds);
        }

        if let (Some(payload), _) = &mut state.outgoing[handle] {
            payload.0.extend_from_slice(buffer);
            return Ok(());
        }

        Err(Error::LateAccess)
    }

    /// Record reply to the current message.
    pub fn reply_commit(
        &mut self,
        packet: ReplyPacket,
    ) -> Result<MessageId, Error> {
        let mut state = self.state.borrow_mut();

        match &mut state.reply {
            (_, Some(_)) => Err(Error::LateAccess),
            (payload, msg) => {
                let mut reply = self.id_generator.borrow_mut().produce_reply(packet);

                let packet_payload = reply.payload.0.clone();
                reply.payload.0.clear();
                reply
                    .payload
                    .0
                    .extend_from_slice(&payload.as_ref().unwrap_or(&Payload::default()).0);
                reply.payload.0.extend_from_slice(&packet_payload);

                *msg = Some(reply);
                *payload = None;

                Ok(msg.as_ref().unwrap().id())
            }
        }
    }

    /// Push an extra buffer into reply message.
    pub fn reply_push(&self, buffer: &[u8]) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();

        match &mut state.reply {
            (_, Some(_)) => { return Err(Error::LateAccess) },
            (Some(payload), _) => payload.0.extend_from_slice(buffer),
            (None, _) => state.reply.0 = Some(buffer.to_vec().into()),
        }

        Ok(())
    }

    /// Add the current message to the wait list.
    pub fn wait(&self) -> Result<(), Error> {
        if self.state.borrow().waiting.is_some() {
            return Err(Error::DuplicateWaiting);
        }
        self.state.borrow_mut().waiting = Some(self.current().clone());
        Ok(())
    }

    /// Mark a message to be woken using `waker_id`.
    pub fn wake(&self, waker_id: MessageId) -> Result<(), Error> {
        if self.state.borrow().awakening.is_some() {
            return Err(Error::DuplicateAwakening);
        }
        self.state.borrow_mut().awakening = Some(waker_id);
        Ok(())
    }

    /// Mark message as fully formed and ready for sending in this context by handle.
    pub fn send_commit(
        &mut self,
        handle: usize,
        packet: OutgoingPacket,
    ) -> Result<MessageId, Error> {
        let mut state = self.state.borrow_mut();

        if handle >= state.outgoing.len() {
            return Err(Error::OutOfBounds);
        }

        match &mut state.outgoing[handle] {
            (None, _) => Err(Error::LateAccess),
            (payload, msg) => {
                let mut outgoing = self.id_generator.borrow_mut().produce_outgoing(packet);

                let packet_payload = outgoing.payload.0.clone();
                outgoing.payload.0.clear();
                outgoing
                    .payload
                    .0
                    .extend_from_slice(&payload.as_ref().unwrap().0);
                outgoing.payload.0.extend_from_slice(&packet_payload);

                *msg = Some(outgoing);
                *payload = None;

                Ok(msg.as_ref().unwrap().id())
            }
        }
    }

    /// Return reference to the current incoming message.
    pub fn current(&self) -> &IncomingMessage {
        self.current.as_ref()
    }

    /// Last used nonce
    pub fn nonce(&self) -> u64 {
        self.id_generator.borrow().current()
    }

    /// Drop this context.
    ///
    /// Do it to return all outgoing messages and optional reply generated using this context.
    pub fn drain(
        self,
    ) -> (
        Vec<OutgoingMessage>,
        Option<ReplyMessage>,
        Option<IncomingMessage>,
        Option<MessageId>,
    ) {
        let Self { state, .. } = self;
        let mut state = Rc::try_unwrap(state)
            .expect("Calling drain with references to the memory context left")
            .into_inner();

        (
            state
                .outgoing
                .drain(..)
                .filter_map(|v| if v.0 == None { v.1 } else { None })
                .collect(),
            state.reply.1,
            state.waiting,
            state.awakening,
        )
    }
}

#[cfg(test)]
/// This module contains tests of the `MessageContext` structure
/// functionality from the `message.rs` module
mod tests {
    use super::*;

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
        let incoming_message = IncomingMessage {
            id: MessageId::from(INCOMING_MESSAGE_ID),
            source: ProgramId::from(INCOMING_MESSAGE_SOURCE),
            payload: vec![1, 2].into(),
            gas_limit: 0,
            value: 0,
            reply: None,
        };

        // Creating a message context
        let mut context = MessageContext::new(incoming_message, id_generator);

        // Сhecking that the initial parameters of the context match the passed constants
        assert_eq!(context.current().id, MessageId::from(INCOMING_MESSAGE_ID));
        assert_eq!(context.nonce(), DEFAULT_NONCE);
        assert!(context.state.borrow_mut().reply.is_none());

        // Creating a reply packet to set the `ReplyMessage`
        let reply_packet = ReplyPacket::new(0, vec![0, 0, 0].into(), 0, 0);

        // Checking that we are not able to push extra payload into
        // reply message if we have not set it yet
        assert!(context.reply_push(&[0]).is_err());

        // Setting reply message and making sure the operation was successful
        assert!(context.reply(reply_packet.clone()).is_ok());

        // After every successful generation of `Message`, `nonse` increases by one
        assert_eq!(context.nonce(), DEFAULT_NONCE + 1);

        // Checking that the `ReplyMessage` mathes the passed one
        assert_eq!(
            context.state.borrow_mut().reply.as_ref().unwrap().payload,
            vec![0, 0, 0].into()
        );

        // Checking that repeated call `reply(...)` returns error and does not
        // increase nonse, because `ReplyMessage` is not generated
        assert!(context.reply(reply_packet.clone()).is_err());
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
        assert!(context.state.borrow_mut().outgoing[expected_handle]
            .0
            .is_some());

        // Checking that we are able to push payload for the
        // message that we have not commited yet
        assert!(context.send_push(expected_handle, &[5, 7]).is_ok());
        assert!(context.send_push(expected_handle, &[9]).is_ok());

        // Creating an outgoing packet to commit sending by parts
        let commit_packet = OutgoingPacket::new(
            ProgramId::from(OUTGOING_MESSAGE_DEST + 1),
            Payload::default(),
            0,
            0,
        );

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
            context.state.borrow().reply.as_ref().unwrap().payload.0,
            vec![0, 0, 0]
        );

        // Checking that we are able to push extra payload into reply message
        assert!(context.reply_push(&[1, 2]).is_ok());
        assert!(context.reply_push(&[3, 4]).is_ok());

        // Checking that on drain we get only messages that were fully formed (directly sent or commited)
        let expected_result = context.drain();
        assert_eq!(expected_result.0.len(), 1);
        assert_eq!(expected_result.0[0].payload.0, vec![5, 7, 9]);

        // Checking that we successfully pushed extra payload into reply
        assert!(expected_result.1.is_some());
        assert_eq!(
            expected_result.1.unwrap().payload.0,
            vec![0, 0, 0, 1, 2, 3, 4]
        );
    }
}
