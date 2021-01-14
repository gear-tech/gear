//! Storage backing abstractions

use std::collections::{HashMap, VecDeque};

use crate::{
    program::{ProgramId, Program},
    message::Message,
};

pub trait ProgramStorage {
    fn get(&self, id: ProgramId) -> Option<&Program>;

    fn get_mut(&mut self, id: ProgramId) -> Option<&mut Program>;

    fn set(&mut self, program: Program) -> Option<Program>;

    fn remove(&mut self, id: ProgramId) -> Option<Program>;
}

pub struct InMemoryProgramStorage {
    inner: HashMap<ProgramId, Program>,
}

impl InMemoryProgramStorage {
    pub fn new(programs: Vec<Program>) -> Self {
        Self { inner: programs.into_iter().map(|p| (p.id(), p)).collect() }
    }
}

impl ProgramStorage for InMemoryProgramStorage {
    fn get(&self, id: ProgramId) -> Option<&Program> {
        self.inner.get(&id)
    }

    fn get_mut(&mut self, id: ProgramId) -> Option<&mut Program> {
        self.inner.get_mut(&id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        self.inner.insert(program.id(), program)
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        self.inner.remove(&id)
    }
}

pub trait MessageQueue {
    fn dequeue(&mut self) -> Option<Message>;

    fn queue(&mut self, message: Message);

    fn queue_many(&mut self, messages: Vec<Message>) {
        for message in messages { self.queue(message) }
    }
}

pub struct InMemoryMessageQueue {
    inner: VecDeque<Message>,    
}

impl InMemoryMessageQueue {
    pub fn new(messages: Vec<Message>) -> Self {
        Self { inner: VecDeque::from(messages) }
    }
}

impl MessageQueue for InMemoryMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        self.inner.pop_front()
    }

    fn queue(&mut self, message: Message) {
        self.inner.push_back(message)
    }
}