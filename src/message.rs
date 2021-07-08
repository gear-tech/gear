//! Message processing module and context.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use crate::program::ProgramId;
use codec::{Decode, Encode};

/// Message payload.
#[derive(Clone, Debug, Decode, Encode, derive_more::From, PartialEq, Eq)]
pub struct Payload(Vec<u8>);

impl Payload {
    /// Return raw bytes of the message payload.
    pub fn into_raw(self) -> Vec<u8> {
        self.0
    }
}

/// Error using messages.
#[derive(Debug)]
pub enum Error {
    /// Message limit exceeded.
    LimitExceeded,
    /// An attempt to commit or to push a payload into an already formed message.
    LateAccess,
    /// No message found with given handle, or handle exceedes the maximum messages amount.
    OutOfBounds,
}

/// Incoming message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    source: ProgramId,
    payload: Payload,
    gas_limit: u64,
    value: u128,
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

    /// Value of the message.
    pub fn value(&self) -> u128 {
        self.value
    }
}

impl From<Message> for IncomingMessage {
    fn from(s: Message) -> Self {
        IncomingMessage {
            source: s.source(),
            payload: s.payload,
            gas_limit: s.gas_limit,
            value: s.value,
        }
    }
}

impl IncomingMessage {
    /// New incomig message from specific `source`, `payload` and `gas_limit`.
    pub fn new(source: ProgramId, payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            source,
            payload,
            gas_limit,
            value,
        }
    }

    /// New system incoming message.
    pub fn new_system(payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            source: ProgramId::system(),
            payload,
            gas_limit,
            value,
        }
    }
}

/// Outgoing message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingMessage {
    dest: ProgramId,
    payload: Payload,
    gas_limit: u64,
    value: u128,
}

impl OutgoingMessage {
    /// New outgoing message.
    pub fn new(dest: ProgramId, payload: Payload, gas_limit: u64, value: u128) -> Self {
        Self {
            dest,
            payload,
            gas_limit,
            value,
        }
    }

    /// Convert outgoing message to the stored message by providing `source`.
    pub fn into_message(self, source: ProgramId) -> Message {
        Message {
            source,
            dest: self.dest,
            payload: self.payload,
            gas_limit: self.gas_limit,
            value: self.value,
        }
    }
}

/// Message.
#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Message {
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
}

impl Message {
    /// New system message to the specific program.
    pub fn new_system(dest: ProgramId, payload: Payload, gas_limit: u64, value: u128) -> Message {
        Message {
            source: 0.into(),
            dest,
            payload,
            gas_limit,
            value,
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
}

/// Message formation status.
#[derive(Debug, PartialEq)]
pub enum FormationStatus {
    /// Message is fully formed and ready to be sent.
    Formed,
    /// Message is not fully formed yet.
    NotFormed,
}

/// Message state of the current session.
///
/// Contains all generated outgoing messages with their formation statuses.
#[derive(Debug)]
pub struct MessageState {
    outgoing: Vec<(OutgoingMessage, FormationStatus)>,
}

/// Message context for the currently running program.
#[derive(Clone)]
pub struct MessageContext {
    state: Rc<RefCell<MessageState>>,
    outgoing_limit: usize,
    current: Rc<IncomingMessage>,
}

impl MessageContext {
    /// New context.
    ///
    /// Create context by providing incoming message for the program.
    pub fn new(incoming_message: IncomingMessage) -> MessageContext {
        MessageContext {
            state: Rc::new(RefCell::new(MessageState { outgoing: vec![] })),
            outgoing_limit: 128,
            current: Rc::new(incoming_message),
        }
    }

    /// Initialize a new message with `NotFormed` formation status and return its handle.
    ///
    /// Messages created this way should be commited with `commit(handle)` to be sent.
    pub fn init(&self, msg: OutgoingMessage) -> Result<usize, Error> {
        let mut state = self.state.borrow_mut();

        let outgoing_count = state.outgoing.len();

        if outgoing_count >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        state.outgoing.push((msg, FormationStatus::NotFormed));

        Ok(outgoing_count)
    }

    /// Push an extra buffer into message payload by handle.
    pub fn push(&self, handle: usize, buffer: &mut Vec<u8>) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();

        if handle >= state.outgoing.len() {
            return Err(Error::OutOfBounds);
        }

        if let (msg, FormationStatus::NotFormed) = &mut state.outgoing[handle] {
            msg.payload.0.append(buffer);
            return Ok(());
        }

        Err(Error::LateAccess)
    }

    /// Mark message as fully formed and ready for sending in this context by handle.
    pub fn commit(&self, handle: usize) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();

        if handle >= state.outgoing.len() {
            return Err(Error::OutOfBounds);
        }

        match &mut state.outgoing[handle] {
            (_, FormationStatus::Formed) => Err(Error::LateAccess),
            (_, status) => {
                *status = FormationStatus::Formed;
                Ok(())
            }
        }
    }

    /// Send fully formed message to another program in this context.
    pub fn send(&self, msg: OutgoingMessage) -> Result<(), Error> {
        let mut state = self.state.borrow_mut();

        if state.outgoing.len() >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        state.outgoing.push((msg, FormationStatus::Formed));

        Ok(())
    }

    /// Return reference to the current incoming message.
    pub fn current(&self) -> &IncomingMessage {
        &self.current.as_ref()
    }

    /// Drop this context.
    ///
    /// Do it to return all messages generated using this context.
    pub fn drain(self) -> Vec<OutgoingMessage> {
        let mut state = self.state.borrow_mut();

        state
            .outgoing
            .drain(..)
            .filter_map(|v| {
                if v.1 == FormationStatus::Formed {
                    Some(v.0)
                } else {
                    None
                }
            })
            .collect()
    }
}
