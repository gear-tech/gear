//! Storage backing abstractions

use hashbrown::HashMap;
use alloc::collections::VecDeque;
use alloc::vec::Vec;


use crate::{
    program::{ProgramId, Program},
    message::Message,
    memory::PageNumber,
};

/// Abstraction over program storage.
pub trait ProgramStorage {
    /// Get the program from the storage.
    fn get(&self, id: ProgramId) -> Option<Program>;

    /// Store program in the storage.
    fn set(&mut self, program: Program) -> Option<Program>;

    /// Remove the program from the storage.
    fn remove(&mut self, id: ProgramId) -> Option<Program>;
}

/// In-memory program storage (for tests).
pub struct InMemoryProgramStorage {
    inner: HashMap<ProgramId, Program>,
}

impl InMemoryProgramStorage {
    /// New in-memory program storage with specified number of programs already set.
    pub fn new(programs: Vec<Program>) -> Self {
        Self { inner: programs.into_iter().map(|p| (p.id(), p)).collect() }
    }

    /// Drop the in-memory storage and return what is stored.
    pub fn drain(self) -> Vec<Program> {
        self.inner.into_iter().map(|(_, program)| program).collect()
    }
}

impl ProgramStorage for InMemoryProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        self.inner.get(&id).cloned()
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        self.inner.insert(program.id(), program)
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        self.inner.remove(&id)
    }
}

/// Message queue storage.
pub trait MessageQueue {
    /// Dequeue next message.
    fn dequeue(&mut self) -> Option<Message>;

    /// Queue message.
    fn queue(&mut self, message: Message);

    /// Queue many messages.
    fn queue_many(&mut self, messages: Vec<Message>) {
        for message in messages { self.queue(message) }
    }
}

/// In-memory message queue (for tests).
pub struct InMemoryMessageQueue {
    inner: VecDeque<Message>,
    log: Vec<Message> // messages sent to /0
}

impl InMemoryMessageQueue {
    /// New in-memory message queue consisting of the provided messages.
    pub fn new(messages: Vec<Message>) -> Self {
        Self { inner: VecDeque::from(messages), log: Vec::new() }
    }

    /// Drop the in-memory message queue returning what was stored in it.
    pub fn drain(self) -> Vec<Message> {
        self.inner.into_iter().collect()
    }

    /// Messages log (messages sent with no destination).
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

/// Allocations storage.
pub trait AllocationStorage {
    /// Get the owner of the speicific page.
    fn get(&self, page: PageNumber) -> Option<ProgramId>;

    /// Remove owner of the speicifc page.
    fn remove(&mut self, page: PageNumber) -> Option<ProgramId>;

    /// Set owner of the speicifc page.
    fn set(&mut self, page: PageNumber, program: ProgramId);

    /// Check if owner of the specific page is set.
    fn exists(&self, id: PageNumber) -> bool {
        self.get(id).is_some()
    }
}

/// In-memory allocation storage.
pub struct InMemoryAllocationStorage {
    inner: HashMap<PageNumber, ProgramId>,
}

impl InMemoryAllocationStorage {
    /// New in-memory allocation storage.
    pub fn new(allocations: Vec<(PageNumber, ProgramId)>) -> Self {
        Self { inner: allocations.into_iter().collect::<HashMap<_, _, _>>() }
    }

    /// Drop the in-memory allocation storage returning what is allocated by what.
    pub fn drain(self) -> Vec<(PageNumber, ProgramId)> {
        self.inner.iter().map(|(page, pid)| (*page, *pid)).collect()
    }
}

impl AllocationStorage for InMemoryAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        self.inner.get(&id).copied()
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        self.inner.remove(&id)
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        self.inner.insert(page, program);
    }
}

/// Storage.
pub struct Storage<AS: AllocationStorage, MQ: MessageQueue, PS: ProgramStorage> {
    /// Allocation storage.
    pub allocation_storage: AS,
    /// Message queue stoage.
    pub message_queue: MQ,
    /// Program storage.
    pub program_storage: PS,
}

/// Fully in-memory storage (for tests).
pub type InMemoryStorage = Storage<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

/// Create new in-memory storage for tests by providing all data.
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
