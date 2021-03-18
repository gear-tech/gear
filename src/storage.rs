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

    pub fn drain(self) -> Vec<Program> {
        self.inner.into_iter().map(|(_, program)| program).collect()
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
    log: Vec<Message> // messages sent to /0
}

impl InMemoryMessageQueue {
    pub fn new(messages: Vec<Message>) -> Self {
        Self { inner: VecDeque::from(messages), log: Vec::new() }
    }

    pub fn drain(self) -> Vec<Message> {
        self.inner.into_iter().collect()
    }

    pub fn log(&self) -> &[Message] {
        &self.log[..]
    }
}

impl MessageQueue for InMemoryMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        self.inner.pop_front()
    }

    fn queue(&mut self, message: Message) {
        if message.dest() == 0.into() {
            self.log.push(message);
            return;
        }

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
}

pub struct InMemoryAllocationStorage {
    inner: HashMap<PageNumber, ProgramId>,
}

impl InMemoryAllocationStorage {
    pub fn new(allocations: Vec<(PageNumber, ProgramId)>) -> Self {
        Self { inner: allocations.into_iter().collect::<HashMap<_, _, _>>() }
    }

    pub fn drain(self) -> Vec<(PageNumber, ProgramId)> {
        self.inner.iter().map(|(page, pid)| (*page, *pid)).collect()
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
}

pub struct Storage<AS: AllocationStorage, MQ: MessageQueue, PS: ProgramStorage> {
    pub allocation_storage: AS,
    pub message_queue: MQ,
    pub program_storage: PS,
}

pub type InMemoryStorage = Storage<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

pub fn new_in_memory(
    allocations: Vec<(PageNumber, ProgramId)>,
    messages: Vec<Message>,
    programs: Vec<Program>,
) -> InMemoryStorage {
    Storage {
        allocation_storage: InMemoryAllocationStorage::new(allocations),
        message_queue: InMemoryMessageQueue::new(messages),
        program_storage: InMemoryProgramStorage::new(programs),
    }
}
