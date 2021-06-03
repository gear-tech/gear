//! Message processing module and context.

use std::{rc::Rc, cell::RefCell};

use codec::{Encode, Decode};
use crate::program::ProgramId;

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
}

/// Incoming message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    source: Option<ProgramId>,
    payload: Payload,
    gas_limit: Option<u64>,
    value: u128,
}

impl IncomingMessage {
    /// Source of the incoming message, if any.
    pub fn source(&self) -> Option<ProgramId> {
        self.source
    }

    /// Payload of the incoming message.
    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }

    /// Gas limit of the message.
    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }

    /// Value of the message
    pub fn value(&self) -> u128 {
        self.value
    }
}

impl From<Message> for IncomingMessage {
    fn from(s: Message) -> Self {
        IncomingMessage {
            source: Some(s.source()),
            payload: s.payload,
            gas_limit: s. gas_limit,
            value: s.value,
        }
    }
}

impl IncomingMessage {
    /// New incomig message from specific `source`, `payload` and `gas_limit`.
    pub fn new(
        source: ProgramId,
        payload: Payload,
        gas_limit: Option<u64>,
        value: u128,
    ) -> Self {
        Self { source: Some(source), payload, gas_limit, value  }
    }

    /// New system incominng messaage.
    pub fn new_system(
        payload: Payload,
        gas_limit: Option<u64>,
        value: u128,
    ) -> Self {
        Self { source: None, payload, gas_limit, value }
    }
}

/// Outgoing message.
#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingMessage {
    dest: ProgramId,
    payload: Payload,
    gas_limit: Option<u64>,
    value: u128,
}

impl OutgoingMessage {
    /// New outgoing message.
    pub fn new(
        dest: ProgramId,
        payload: Payload,
        gas_limit: Option<u64>,
        value: u128,
    ) -> Self {
        Self { dest, payload, gas_limit, value }
    }

    /// Convert outgoing message to the stored message by providing `source`
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
    pub gas_limit: Option<u64>,
    /// Message value
    pub value: u128,
}

impl Message {
    /// New system message to the specific program.
    pub fn new_system(
        dest: ProgramId,
        payload: Payload,
        gas_limit: Option<u64>,
        value: u128,
    ) -> Message {
        Message { source: 0.into(), dest, payload, gas_limit, value }
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
    pub fn gas_limit(&self) -> Option<u64> {
        self.gas_limit
    }

    /// Message vaue
    pub fn value(&self) -> u128 {
        self.value
    }
}

/// Message state of the current session.
///
/// Contains all generated outgoing messages.
#[derive(Debug)]
pub struct MessageState {
    outgoing: Vec<OutgoingMessage>,
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
            state: Rc::new(RefCell::new(
                MessageState {
                    outgoing: vec![],
                }
            )),
            outgoing_limit: 128,
            current: Rc::new(incoming_message),
        }
    }

    /// Send message to another program in this context.
    pub fn send(&self, msg: OutgoingMessage) -> Result<(), Error> {
        if self.state.borrow().outgoing.len() >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        self.state.borrow_mut().outgoing.push(msg);

        Ok(())
    }

    /// Return reference to the current incoming message.
    pub fn current(&self) -> &IncomingMessage {
        &self.current.as_ref()
    }

    /// Drop this context.
    ///
    /// Do it to retur nall message generated using this context.
    pub fn drain(self) -> Vec<OutgoingMessage> {
        let Self { state, .. } = self;
        let mut st = state.borrow_mut();

        st.outgoing.drain(..).collect()
    }
}
