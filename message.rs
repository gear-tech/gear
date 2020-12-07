use std::{rc::Rc, cell::RefCell};

use codec::{Encode, Decode};
use crate::program::ProgramId;

#[derive(Clone, Debug, Decode, Encode, derive_more::From)]
pub struct Payload(Vec<u8>);

#[derive(Debug)]
pub enum Error {
    LimitExceeded,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct IncomingMessage {
    source: Option<ProgramId>,
    payload: Payload,
}

impl IncomingMessage {
    pub fn source(&self) -> Option<ProgramId> {
        self.source
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }
}

impl From<Message> for IncomingMessage {
    fn from(s: Message) -> Self {
        IncomingMessage {
            source: Some(s.source()),
            payload: s.payload,
        }
    }
}

impl IncomingMessage {
    pub fn new(source: ProgramId, payload: Payload) -> Self {
        Self { source: Some(source), payload }
    }

    pub fn new_system(payload: Payload) -> Self {
        Self { source: None, payload }
    }
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct OutgoingMessage {
    dest: ProgramId,
    payload: Payload,
}

impl OutgoingMessage {
    pub fn new(dest: ProgramId, payload: Payload) -> Self {
        Self { dest, payload }
    }

    pub fn into_message(self, source: ProgramId) -> Message {
        Message { source, dest: self.dest, payload: self.payload }
    }
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct Message {
    pub source: ProgramId,
    pub dest: ProgramId,
    pub payload: Payload,
}

impl Message {
    pub fn dest(&self) -> ProgramId {
        self.dest
    }

    pub fn source(&self) -> ProgramId {
        self.source
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload.0[..]
    }
}

#[derive(Debug)]
pub struct MessageState {
    outgoing: Vec<OutgoingMessage>,
}

#[derive(Clone)]
pub struct MessageContext {
    program_id: ProgramId,
    state: Rc<RefCell<MessageState>>,
    outgoing_limit: usize,
    current: Rc<IncomingMessage>,
}

impl MessageContext {
    pub fn new(program_id: ProgramId, incoming_message: IncomingMessage) -> MessageContext {
        MessageContext {
            program_id,
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
        if !(self.state.borrow().outgoing.len() < self.outgoing_limit) {
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
