use std::{rc::Rc, cell::RefCell};

use codec::{Encode, Decode};
use crate::program::ProgramId;

#[derive(Clone, Debug, Decode, Encode, derive_more::From, PartialEq, Eq)]
pub struct Payload(Vec<u8>);

impl Payload {
    pub fn into_raw(self) -> Vec<u8> {
        self.0
    }
}

#[derive(Debug)]
pub enum Error {
    LimitExceeded,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    source: Option<ProgramId>,
    payload: Payload,
    gas_limit: u64,
}

impl IncomingMessage {
    pub fn source(&self) -> Option<ProgramId> {
        self.source
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }

    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
}

impl From<Message> for IncomingMessage {
    fn from(s: Message) -> Self {
        IncomingMessage {
            source: Some(s.source()),
            payload: s.payload,
            gas_limit: s. gas_limit,
        }
    }
}

impl IncomingMessage {
    pub fn new(source: ProgramId, payload: Payload, gas_limit: u64) -> Self {
        Self { source: Some(source), payload, gas_limit  }
    }

    pub fn new_system(payload: Payload, gas_limit: u64) -> Self {
        Self { source: None, payload, gas_limit }
    }
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingMessage {
    dest: ProgramId,
    payload: Payload,
    gas_limit: u64,
}

impl OutgoingMessage {
    pub fn new(dest: ProgramId, payload: Payload, gas_limit: u64) -> Self {
        Self { dest, payload, gas_limit }
    }

    pub fn into_message(self, source: ProgramId) -> Message {
        Message { source, dest: self.dest, payload: self.payload, gas_limit: self.gas_limit }
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub struct Message {
    pub source: ProgramId,
    pub dest: ProgramId,
    pub payload: Payload,
    pub gas_limit: u64,
}

impl Message {
    pub fn new_system(dest: ProgramId, payload: Payload, gas_limit: u64) -> Message {
        Message { source: 0.into(), dest, payload, gas_limit }
    }

    pub fn dest(&self) -> ProgramId {
        self.dest
    }

    pub fn source(&self) -> ProgramId {
        self.source
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }

    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }
}

#[derive(Debug)]
pub struct MessageState {
    outgoing: Vec<OutgoingMessage>,
}

#[derive(Clone)]
pub struct MessageContext {
    state: Rc<RefCell<MessageState>>,
    outgoing_limit: usize,
    current: Rc<IncomingMessage>,
}

impl MessageContext {
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

    pub fn send(&self, msg: OutgoingMessage) -> Result<(), Error> {
        if self.state.borrow().outgoing.len() >= self.outgoing_limit {
            return Err(Error::LimitExceeded);
        }

        self.state.borrow_mut().outgoing.push(msg);

        Ok(())
    }

    pub fn current(&self) -> &IncomingMessage {
        &self.current.as_ref()
    }

    pub fn drain(self) -> Vec<OutgoingMessage> {
        let Self { state, .. } = self;
        let mut st = state.borrow_mut();

        st.outgoing.drain(..).collect()
    }
}
