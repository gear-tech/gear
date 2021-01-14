//! Storage backing abstractions

use std::collections::{HashMap, VecDeque};

use crate::{
    program::{ProgramId, Program},
    message::Message,
    memory::PageNumber,
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

pub trait AllocationStorage {
    fn get(&self, page: PageNumber) -> Option<&ProgramId>;

    fn remove(&mut self, page: PageNumber) -> Option<ProgramId>;

    fn set(&mut self, page: PageNumber, program: ProgramId);

    fn exists(&self, id: PageNumber) -> bool {
        self.get(id).is_some()
    }

    fn count(&self) -> usize;

    fn clear(&mut self, program_id: ProgramId);

    fn query(&self) -> Vec<(PageNumber, ProgramId)>;
}

pub struct InMemoryAllocationStorage {
    inner: HashMap<PageNumber, ProgramId>,
}

impl InMemoryAllocationStorage {
    pub fn new(allocations: Vec<(PageNumber, ProgramId)>) -> Self {
        Self { inner: allocations.into_iter().collect::<HashMap<_, _, _>>() }  
    }
}

impl AllocationStorage for InMemoryAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<&ProgramId> {
        self.inner.get(&id)
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        self.inner.remove(&id)
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        self.inner.insert(page, program);
    }

    fn count(&self) -> usize {
        self.inner.len()
    }

    fn clear(&mut self, program_id: ProgramId) {
        self.inner.retain(|_, pid| *pid != program_id);
    }

    fn query(&self) -> Vec<(PageNumber, ProgramId)> {
        self.inner.iter().map(|(page, pid)| (*page, *pid)).collect()
    }
}