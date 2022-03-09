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

//! Message processing module and context.

use crate::identifiers::*;
use alloc::{collections::BTreeMap, rc::Rc, vec::Vec};
use codec::{Decode, Encode};
use core::cell::RefCell;
use scale_info::TypeInfo;

/// Payload type for message.
pub type Payload = Vec<u8>;

/// Gas limit type for message.
pub type GasLimit = u64;

/// Value type for message.
pub type Value = u128;

/// Exit code type for message replies.
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
    /// Duplicate waking message.
    DuplicateWaking,
    /// An attempt to commit or to push a payload into an already formed message.
    LateAccess,
    /// No message found with given handle, or handle exceeds the maximum messages amount.
    OutOfBounds,
    /// An attempt to push a payload into reply that was not set
    NoReplyFound,
    /// An attempt to interrupt execution with `wait(..)` while some messages weren't completed
    UncommittedPayloads,
    /// Duplicate init message
    DuplicateInit,
}

/// Message.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct Message {
    id: MessageId,
    source: ProgramId,
    destination: ProgramId,
    payload: Payload,
    gas_limit: Option<GasLimit>,
    value: Value,
    reply: Option<(MessageId, ExitCode)>,
}

impl Message {
    /// New message from user
    pub fn new(
        user_id: ProgramId,
        block_number: u32,
        local_nonce: u32,
        destination: ProgramId,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        let id = MessageId::generate_from_user(block_number, user_id, local_nonce);
        Self {
            id,
            source: user_id,
            destination,
            payload,
            gas_limit,
            value,
            reply: None,
        }
    }

    /// New reply message from user
    pub fn new_reply(
        user_id: ProgramId,
        destination: ProgramId,
        payload: Payload,
        gas_limit: GasLimit,
        value: Value,
        reply_to: MessageId,
    ) -> Self {
        let id = MessageId::generate_reply(reply_to, 0);
        Self {
            id,
            source: user_id,
            destination,
            payload,
            gas_limit: Some(gas_limit),
            value,
            reply: Some((reply_to, 0)),
        }
    }

    /// Id of the message.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Source of the message.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Destination of the message.
    pub fn destination(&self) -> ProgramId {
        self.destination
    }

    /// Payload of the message.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> Option<GasLimit> {
        self.gas_limit
    }

    /// Value of the message.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message id what this message replies to.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply.map(|(id, _)| id)
    }

    /// Exit code of the message.
    pub fn exit_code(&self) -> Option<ExitCode> {
        self.reply.map(|(_, exit_code)| exit_code)
    }

    /// Check if this message is reply.
    pub fn is_reply(&self) -> bool {
        self.reply.is_some()
    }
}

/// Incoming message.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct IncomingMessage {
    id: MessageId,
    source: ProgramId,
    payload: Payload,
    gas_limit: GasLimit,
    value: Value,
    reply: Option<(MessageId, ExitCode)>,
}

impl IncomingMessage {
    /// From Message constructor.
    pub fn from_message(msg: Message, gas_limit: GasLimit) -> Self {
        Self {
            id: msg.id,
            source: msg.source,
            payload: msg.payload,
            gas_limit,
            value: msg.value,
            reply: msg.reply,
        }
    }

    /// Id of the message.
    pub fn id(&self) -> MessageId {
        self.id
    }

    /// Source of the message.
    pub fn source(&self) -> ProgramId {
        self.source
    }

    /// Payload of the message.
    pub fn payload(&self) -> &[u8] {
        self.payload.as_ref()
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> GasLimit {
        self.gas_limit
    }

    /// Value of the message.
    pub fn value(&self) -> Value {
        self.value
    }

    /// Message id what this message replies to.
    pub fn reply_to(&self) -> Option<MessageId> {
        self.reply.map(|(id, _)| id)
    }

    /// Exit code of the message.
    pub fn exit_code(&self) -> Option<ExitCode> {
        self.reply.map(|(_, exit_code)| exit_code)
    }

    /// Check if this message is reply.
    pub fn is_reply(&self) -> bool {
        self.reply.is_some()
    }
}

/// Outgoing message packet.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct OutgoingPacket {
    destination: ProgramId,
    payload: Payload,
    gas_limit: Option<GasLimit>,
    value: Value,
}

impl OutgoingPacket {
    /// New outgoing message packet constructor.
    pub fn new(
        destination: ProgramId,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        Self {
            destination,
            payload,
            gas_limit,
            value,
        }
    }
}

/// Outgoing message.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct OutgoingMessage {
    id: MessageId,
    packet: OutgoingPacket,
}

impl OutgoingMessage {
    /// New outgoing message.
    pub fn new(origin_msg_id: MessageId, local_nonce: u8, packet: OutgoingPacket) -> Self {
        let id = MessageId::generate_outgoing(origin_msg_id, local_nonce);
        Self { id, packet }
    }

    /// Convert outgoing message to message.
    pub fn into_message(self, source: ProgramId) -> Message {
        Message {
            id: self.id,
            source,
            destination: self.packet.destination,
            payload: self.packet.payload,
            gas_limit: self.packet.gas_limit,
            value: self.packet.value,
            reply: None,
        }
    }
}

/// Reply message packet.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct ReplyPacket {
    payload: Payload,
    gas_limit: Option<GasLimit>,
    value: Value,
}

impl ReplyPacket {
    /// New reply message packet constructor.
    pub fn new(payload: Payload, gas_limit: Option<GasLimit>, value: Value) -> Self {
        Self {
            payload,
            gas_limit,
            value,
        }
    }
}

/// Reply message.
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct ReplyMessage {
    id: MessageId,
    origin_msg_id: MessageId,
    exit_code: ExitCode,
    packet: ReplyPacket,
}

impl ReplyMessage {
    /// New reply message.
    pub fn new(origin_msg_id: MessageId, exit_code: ExitCode, packet: ReplyPacket) -> Self {
        let id = MessageId::generate_reply(origin_msg_id, exit_code);
        Self {
            id,
            origin_msg_id,
            exit_code,
            packet,
        }
    }

    /// New system reply message.
    pub fn system(origin_msg_id: MessageId, exit_code: ExitCode) -> Self {
        let id = MessageId::generate_reply(origin_msg_id, exit_code);
        let packet = ReplyPacket::new(Default::default(), None, 0);
        Self {
            id,
            origin_msg_id,
            exit_code,
            packet,
        }
    }

    /// Convert reply message to message.
    pub fn into_message(self, source: ProgramId, destination: ProgramId) -> Message {
        Message {
            id: self.id,
            source,
            destination,
            payload: self.packet.payload,
            gas_limit: self.packet.gas_limit,
            value: self.packet.value,
            reply: Some((self.origin_msg_id, self.exit_code)),
        }
    }
}

/// Program initialization message packet
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct InitPacket {
    new_program_id: ProgramId,
    code_hash: CodeId,
    salt: Vec<u8>,
    payload: Payload,
    gas_limit: Option<GasLimit>,
    value: Value,
}

impl InitPacket {
    /// New program initialization message packet constructor.
    pub fn new(
        code_hash: CodeId,
        salt: Vec<u8>,
        payload: Payload,
        gas_limit: Option<GasLimit>,
        value: Value,
    ) -> Self {
        let new_program_id = ProgramId::generate(code_hash, &salt);
        Self {
            new_program_id,
            code_hash,
            salt,
            payload,
            gas_limit,
            value,
        }
    }
}

/// Program initialization message
#[derive(
    Clone,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    codec::Decode,
    codec::Encode,
    scale_info::TypeInfo,
)]
pub struct InitMessage {
    id: MessageId,
    packet: InitPacket,
}

impl InitMessage {
    /// New program initialization message.
    pub fn new(origin_msg_id: MessageId, local_nonce: u8, packet: InitPacket) -> Self {
        let id = MessageId::generate_outgoing(origin_msg_id, local_nonce);
        Self { id, packet }
    }

    /// Convert program initialization message to message.
    pub fn into_message(self, source: ProgramId) -> Message {
        Message {
            id: self.id,
            source,
            destination: self.packet.new_program_id,
            payload: self.packet.payload,
            gas_limit: self.packet.gas_limit,
            value: self.packet.value,
            reply: None,
        }
    }
}

/// Message state of the current session.
///
/// Contains all generated outgoing messages with their formation statuses.
#[derive(Debug, Default)]
pub struct MessageState {
    /// Collection of outgoing messages generated.
    pub outgoing: Vec<OutgoingMessage>,
    /// Collection of init messages for new programs generated.
    pub init_messages: Vec<InitMessage>,
    /// Reply generated.
    pub reply: Option<ReplyMessage>,
    /// Messages to be waken.
    pub awakening: Vec<MessageId>,
}

/// Pushed payloads of current message processing.
#[derive(Encode, Decode, TypeInfo, Debug, Default, Clone, PartialEq)]
pub struct PayloadStore {
    /// Outgoing payloads ever formed for current message processing.
    pub outgoing: BTreeMap<u64, Option<Payload>>,
    /// Program ids of newly created programs in the current message processing.
    pub new_programs: Vec<ProgramId>,
    /// Reply payload ever formed for current message processing.
    pub reply: Option<Payload>,
    /// Messages were ever waken for current message processing.
    pub awaken: Vec<MessageId>,
    /// Flag were reply ever sent for current message processing.
    pub reply_was_sent: bool,
}

/// Message context for the currently running program.
#[derive(Clone)]
pub struct MessageContext {
    state: Rc<RefCell<MessageState>>,
    store: Rc<RefCell<PayloadStore>>,
    outgoing_limit: u64,
    current: Rc<IncomingMessage>,
}

impl MessageContext {
    /// New context.
    ///
    /// Create context by providing incoming message for the program.
    pub fn new(incoming_message: IncomingMessage, store: Option<PayloadStore>) -> MessageContext {
        MessageContext {
            state: Default::default(),
            store: store.map(|v| Rc::new(RefCell::from(v))).unwrap_or_default(),
            outgoing_limit: 128,
            current: Rc::new(incoming_message),
        }
    }

    /// Mark message as fully formed and ready for sending in this context by handle.
    pub fn send_commit(
        &mut self,
        handle: usize,
        packet: OutgoingPacket,
    ) -> Result<MessageId, Error> {
        if let Some(payload) = self.store.borrow_mut().outgoing.get_mut(&(handle as u64)) {
            if let Some(data) = payload.take() {
                let mut outgoing = self.id_generator.borrow_mut().produce_outgoing(packet);
                outgoing.payload.0.splice(0..0, data.0);
                let id = outgoing.id();
                let mut state = self.state.borrow_mut();
                state.outgoing.push(outgoing);
                Ok(id)
            } else {
                Err(Error::LateAccess)
            }
        } else {
            Err(Error::OutOfBounds)
        }
    }

    /// Initialize a new message with `NotFormed` formation status and return its handle.
    ///
    /// Messages created this way should be committed with `commit(handle)` to be sent.
    pub fn send_init(&mut self) -> Result<usize, Error> {
        let mut store = self.store.borrow_mut();

        let len = store.outgoing.len() as u64;

        if len >= self.outgoing_limit {
            Err(Error::LimitExceeded)
        } else {
            store.outgoing.insert(len, Some(Default::default()));
            Ok(len as usize)
        }
    }

    /// Push an extra buffer into message payload by handle.
    pub fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Error> {
        match self.store.borrow_mut().outgoing.get_mut(&(handle as u64)) {
            Some(Some(payload)) => {
                payload.0.extend_from_slice(buffer);
                Ok(())
            }
            Some(None) => Err(Error::LateAccess),
            None => Err(Error::OutOfBounds),
        }
    }

    /// Record reply to the current message.
    pub fn reply_commit(&mut self, packet: ReplyPacket) -> Result<MessageId, Error> {
        let mut store = self.store.borrow_mut();

        if store.reply_was_sent {
            Err(Error::DuplicateReply)
        } else {
            let mut reply = self.id_generator.borrow_mut().produce_reply(packet);
            let stored_payload = store.reply.take().unwrap_or_default();
            reply.payload.0.splice(0..0, stored_payload.0);

            let id = reply.id();
            self.state.borrow_mut().reply = Some(reply);
            store.reply_was_sent = true;
            Ok(id)
        }
    }

    /// Push an extra buffer into reply message.
    pub fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Error> {
        let mut store = self.store.borrow_mut();

        if store.reply_was_sent {
            Err(Error::LateAccess)
        } else {
            let reply_payload = store.reply.get_or_insert_with(Default::default);
            reply_payload.0.extend_from_slice(buffer);
            Ok(())
        }
    }

    /// Mark a message to be woken using `waker_id`.
    pub fn wake(&self, waker_id: MessageId) -> Result<(), Error> {
        let mut store = self.store.borrow_mut();

        if store.awaken.contains(&waker_id) {
            Err(Error::DuplicateWaking)
        } else {
            store.awaken.push(waker_id);
            self.state.borrow_mut().awakening.push(waker_id);
            Ok(())
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

    /// Convert this context into the message state.
    ///
    /// Do it to return all outgoing, reply, waiting, ane awakening messages generated using this context.
    pub fn drain(self) -> (MessageState, PayloadStore) {
        let Self { state, store, .. } = self;

        let state = Rc::try_unwrap(state)
            .expect("Calling drain with references to the memory context left")
            .into_inner();

        let store = Rc::try_unwrap(store)
            .expect("Calling drain with references to the memory context left")
            .into_inner();

        (state, store)
    }

    /// Send a new init program message
    ///
    /// Generates a new program id from provided `packet` data and returns it
    /// along with init message id.
    pub fn send_init_program(
        &mut self,
        packet: ProgramInitPacket,
    ) -> Result<(ProgramId, MessageId), Error> {
        let code_hash = packet.code_hash;
        let new_program_id = ProgramId::generate(code_hash, &packet.salt);

        {
            let payload_store = self.store.borrow();
            if payload_store
                .new_programs
                .iter()
                .any(|id| id == &new_program_id)
            {
                return Err(Error::DuplicateInit);
            }
        }

        let msg = self
            .id_generator
            .borrow_mut()
            .produce_init(new_program_id, packet);
        let msg_id = msg.id;
        self.state.borrow_mut().init_messages.push(msg);
        self.store.borrow_mut().new_programs.push(new_program_id);

        Ok((new_program_id, msg_id))
    }
}

//  /// New message from user
//  pub fn new(
//     user_id: ProgramId,
//     block_number: u32,
//     local_nonce: u32,
//     destination: ProgramId,
//     payload: Payload,
//     value: Value,
// ) -> Self {
//     let id = MessageId::generate_from_user(block_number, user_id, local_nonce);
//     Self {
//         id,
//         source: user_id,
//         destination,
//         payload,
//         value,
//         reply: None,
//     }
// }

// /// New reply message from user
// pub fn new_reply(
//     user_id: ProgramId,
//     destination: ProgramId,
//     payload: Payload,
//     value: Value,
//     reply_to: MessageId,
// ) -> Self {
//     let id = MessageId::generate_reply(reply_to, 0);
//     Self {
//         id,
//         source: user_id,
//         destination,
//         payload,
//         value,
//         reply: Some((reply_to, 0)),
//     }
// }

/// Dispatch.
///
/// Message plus information of entry point.
#[derive(Clone, Debug, PartialEq)]
pub struct Dispatch {
    /// Kind of dispatch.
    pub kind: DispatchKind,
    /// Message to be dispatched.
    pub message: Message,
    /// Payload store related to this dispatch.
    pub payload_store: Option<PayloadStore>,
}

impl Dispatch {
    /// Create init dispatch
    pub fn new_init(message: Message) -> Self {
        let kind = DispatchKind::Init;
        let payload_store: Option<PayloadStore> = None;

        Dispatch {
            message,
            kind,
            payload_store,
        }
    }

    /// Create handle dispatch
    pub fn new_handle(message: Message) -> Self {
        let kind = DispatchKind::Handle;
        let payload_store: Option<PayloadStore> = None;

        Dispatch {
            message,
            kind,
            payload_store,
        }
    }

    /// Create handle reply dispatch
    pub fn new_reply(message: Message) -> Self {
        let kind = DispatchKind::HandleReply;
        let payload_store: Option<PayloadStore> = None;

        Dispatch {
            message,
            kind,
            payload_store,
        }
    }
}

/// Type of wasm execution entry point.
#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, TypeInfo)]
pub enum DispatchKind {
    /// Initialization.
    Init,
    /// Handle.
    Handle,
    /// Handle reply.
    HandleReply,
}

impl DispatchKind {
    /// Convert into entry point (function name).
    pub fn into_entry(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Handle => "handle",
            Self::HandleReply => "handle_reply",
        }
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
            let mut data: Vec<u8> = self.program_id.as_ref().to_vec();
            data.push(self.nonce as u8);
            data.remove(0);

            self.nonce += 1;

            data[..].into()
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
        let mut context = MessageContext::new(incoming_message, id_generator, None);

        // Checking that the initial parameters of the context match the passed constants
        assert_eq!(context.current().id, MessageId::from(INCOMING_MESSAGE_ID));
        assert_eq!(context.nonce(), DEFAULT_NONCE);
        assert!(context.store.borrow().reply.is_none());
        assert!(context.state.borrow().reply.is_none());

        // Creating a reply packet
        let reply_packet = ReplyPacket::new(0, vec![0, 0].into(), 0);

        // Checking that we are able to initialize reply
        assert!(context.reply_push(&[1, 2, 3]).is_ok());

        // Setting reply message and making sure the operation was successful
        assert!(context.reply_commit(reply_packet.clone()).is_ok());

        // After every successful generation of `Message`, `nonse` increases by one
        assert_eq!(context.nonce(), DEFAULT_NONCE + 1);

        // Checking that the `ReplyMessage` matches the passed one
        assert_eq!(
            context.state.borrow().reply.as_ref().unwrap().payload,
            vec![1, 2, 3, 0, 0].into()
        );

        // Checking that repeated call `reply_push(...)` returns error and does not do anything
        assert!(context.reply_push(&[1]).is_err());
        assert_eq!(
            context.state.borrow().reply.as_ref().unwrap().payload,
            vec![1, 2, 3, 0, 0].into()
        );

        // Checking that repeated call `reply_commit(...)` returns error and does not
        // increase nonce, because `ReplyMessage` is not generated
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
        assert!(context
            .store
            .borrow()
            .outgoing
            .get(&(expected_handle as u64))
            .expect("This key should be")
            .is_some());

        // Checking that we are able to push payload for the
        // message that we have not committed yet
        assert!(context.send_push(expected_handle, &[5, 7]).is_ok());
        assert!(context.send_push(expected_handle, &[9]).is_ok());

        // Creating an outgoing packet to commit sending by parts
        let commit_packet = OutgoingPacket::new(
            ProgramId::from(OUTGOING_MESSAGE_DEST + 1),
            Payload::default(),
            Some(0),
            0,
        );

        // Checking if commit is successful
        assert!(context.send_commit(expected_handle, commit_packet).is_ok());

        // Checking that we are **NOT** able to push payload for the message or
        // commit it if we already committed it or directly pushed before
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
            vec![1, 2, 3, 0, 0]
        );

        // Checking that on drain we get only messages that were fully formed (directly sent or committed)
        let (expected_result, _) = context.drain();
        assert_eq!(expected_result.outgoing.len(), 1);
        assert_eq!(expected_result.outgoing[0].payload.0, vec![5, 7, 9]);
    }
}
