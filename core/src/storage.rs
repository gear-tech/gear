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

//! Storage backing abstractions

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use hashbrown::HashMap;

use crate::{
    message::{Message, MessageId},
    program::{Program, ProgramId},
};

/// Abstraction over program storage.
pub trait ProgramStorage: Default {
    /// Get the program from the storage.
    fn get(&self, id: ProgramId) -> Option<Program>;

    /// Store program in the storage.
    fn set(&mut self, program: Program) -> Option<Program>;

    /// Check if program exists.
    fn exists(&self, id: ProgramId) -> bool;

    /// Remove the program from the storage.
    fn remove(&mut self, id: ProgramId) -> Option<Program>;
}

/// In-memory program storage (for tests).
#[derive(Default)]
pub struct InMemoryProgramStorage {
    inner: HashMap<ProgramId, Program>,
}

impl InMemoryProgramStorage {
    /// Create an empty in-memory program storage.
    pub fn new() -> Self {
        Default::default()
    }
}

impl ProgramStorage for InMemoryProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        self.inner.get(&id).cloned()
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        self.inner.insert(program.id(), program)
    }

    fn exists(&self, id: ProgramId) -> bool {
        self.inner.contains_key(&id)
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        self.inner.remove(&id)
    }
}

impl From<Vec<Program>> for InMemoryProgramStorage {
    fn from(programs: Vec<Program>) -> Self {
        Self {
            inner: programs.into_iter().map(|p| (p.id(), p)).collect(),
        }
    }
}

impl From<InMemoryProgramStorage> for Vec<Program> {
    fn from(storage: InMemoryProgramStorage) -> Vec<Program> {
        storage
            .inner
            .into_iter()
            .map(|(_, program)| program)
            .collect()
    }
}

/// Message queue storage.
pub trait MessageQueue: Default {
    /// Dequeue next message.
    fn dequeue(&mut self) -> Option<Message>;

    /// Queue message.
    fn queue(&mut self, message: Message);

    /// Queue many messages.
    fn queue_many(&mut self, messages: Vec<Message>) {
        messages.into_iter().for_each(|m| self.queue(m));
    }
}

/// In-memory message queue (for tests).
#[derive(Default, Debug)]
pub struct InMemoryMessageQueue {
    inner: VecDeque<Message>,
}

impl InMemoryMessageQueue {
    /// Create an empty in-memory message queue.
    pub fn new() -> Self {
        Default::default()
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

impl From<Vec<Message>> for InMemoryMessageQueue {
    fn from(messages: Vec<Message>) -> Self {
        Self {
            inner: VecDeque::from(messages),
        }
    }
}

impl From<InMemoryMessageQueue> for Vec<Message> {
    fn from(queue: InMemoryMessageQueue) -> Vec<Message> {
        queue.inner.into()
    }
}

/// Message map with id as a key.
pub type MessageMap = BTreeMap<(ProgramId, MessageId), Message>;

/// Wait list for suspended messages.
pub trait WaitList: Default {
    /// Insert a message to the wait list.
    fn insert(&mut self, program_id: ProgramId, msg_id: MessageId, message: Message);

    /// Remove the message from the wait list and return it if any.
    fn remove(&mut self, program_id: ProgramId, id: MessageId) -> Option<Message>;
}

/// In-memory wait list (for tests).
#[derive(Default)]
pub struct InMemoryWaitList {
    inner: MessageMap,
}

impl InMemoryWaitList {
    /// New in-memory wait list.
    pub fn new() -> Self {
        Default::default()
    }
}

impl WaitList for InMemoryWaitList {
    fn insert(&mut self, program_id: ProgramId, id: MessageId, message: Message) {
        self.inner.insert((program_id, id), message);
    }

    fn remove(&mut self, program_id: ProgramId, id: MessageId) -> Option<Message> {
        self.inner.remove(&(program_id, id))
    }
}

impl From<MessageMap> for InMemoryWaitList {
    fn from(map: MessageMap) -> Self {
        Self { inner: map }
    }
}

impl From<InMemoryWaitList> for MessageMap {
    fn from(wait_list: InMemoryWaitList) -> MessageMap {
        wait_list.inner
    }
}

/// Log.
#[derive(Default, Debug)]
pub struct Log {
    inner: Vec<Message>,
}

impl Log {
    /// Put message to log.
    pub fn put(&mut self, message: Message) {
        self.inner.push(message)
    }

    /// Get all messages in log.
    pub fn get(&self) -> &[Message] {
        &self.inner
    }
}

/// Storage.
#[derive(Default)]
pub struct Storage<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList> {
    /// Message queue stoage.
    pub message_queue: MQ,
    /// Program storage.
    pub program_storage: PS,
    /// Wait list.
    pub wait_list: WL,
    /// Log.
    pub log: Log,
}

impl<MQ: MessageQueue, PS: ProgramStorage, WL: WaitList> Storage<MQ, PS, WL> {
    /// Create an empty storage.
    pub fn new() -> Self {
        Default::default()
    }

    /// Create a storage from messages queue, programs storage and wait list.
    pub fn from_components(
        message_queue: MQ,
        program_storage: PS,
        wait_list: WL,
        log: Log,
    ) -> Self {
        Self {
            message_queue,
            program_storage,
            wait_list,
            log,
        }
    }
}

/// Fully in-memory storage (for tests).
pub type InMemoryStorage = Storage<InMemoryMessageQueue, InMemoryProgramStorage, InMemoryWaitList>;

#[cfg(test)]
/// This module contains tests of parts of InMemoryStorage:
/// of allocation storage, message queue storage and program storage
mod tests {
    extern crate wabt;
    use super::*;
    use alloc::vec;

    fn parse_wat(source: &str) -> Vec<u8> {
        let module_bytes = wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec();
        module_bytes
    }

    #[test]
    /// Test that InMemoryProgramStorage works correctly
    fn program_storage_interaction() {
        let wat = r#"
            (module
                (import "env" "gr_reply_to"  (func $gr_reply_to (param i32)))
                (import "env" "memory" (memory 2))
                (export "handle" (func $handle))
                (export "handle_reply" (func $handle))
                (export "init" (func $init))
                (func $handle
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $handle_reply
                    i32.const 65536
                    call $gr_reply_to
                )
                (func $init)
            )"#;

        let binary: Vec<u8> = parse_wat(wat);

        // Initialization of some ProgramIds
        let id1 = ProgramId::from(1);

        let id2 = ProgramId::from(2);

        let id3 = ProgramId::from(3);

        // Initialization of InMemoryProgramStorage with our custom vec<Program>
        let mut program_storage: InMemoryProgramStorage = vec![
            Program::new(id1, binary.clone(), Default::default()).expect("err create program"),
            Program::new(id2, binary.clone(), Default::default()).expect("err create program"),
        ]
        .into();

        // Сhecking that the Program with id2 exists in the storage
        // and it is the one that we put
        assert!(program_storage.get(id2).is_some());
        assert_eq!(program_storage.get(id2).unwrap().code(), binary);

        // Сhecking that the Program with id3 does not exist in the storage
        assert!(program_storage.get(id3).is_none());

        // Сhecking that we are able to correctly remove
        // the Program with id2 from storage
        program_storage.remove(id2);
        assert!(program_storage.get(id2).is_none());

        // Сhecking that we are able to correctly set
        // the new Program with id3 in storage
        program_storage
            .set(Program::new(id3, binary, Default::default()).expect("err create program"));
        assert!(program_storage.get(id3).is_some());

        // Сhecking that the storage after all our interactions
        // contains two programs with id1 and id3 and returns them on draining
        let remaining_programs: Vec<Program> = program_storage.into();
        assert_eq!(remaining_programs.len(), 2);

        for program in remaining_programs {
            assert!(program.id() == id1 || program.id() == id3);
        }
    }

    #[test]
    /// Test that InMemoryMessageQueue works correctly
    fn message_queue_interaction() {
        use crate::message::Payload;

        // Initialization of empty InMemoryMessageQueue
        let mut message_queue = InMemoryMessageQueue::new();

        // Сhecking that the storage totally empty
        assert!(message_queue.dequeue().is_none());

        // Addition of multiple messages
        message_queue.queue_many(vec![
            Message::new_system(
                0.into(),
                ProgramId::from(1),
                Payload::from(vec![1]),
                128,
                512,
            ),
            Message::new_system(
                1.into(),
                ProgramId::from(2),
                Payload::from(vec![2]),
                128,
                1024,
            ),
        ]);

        // Сhecking that the first message in queue is the one that we added first
        let msg = message_queue
            .dequeue()
            .expect("An error occurred during unwraping front queue message");

        assert_eq!(msg.dest(), ProgramId::from(1));

        // Сhecking that the message queue after all our interactions
        // contains the only one message the we added last
        let remaining_messages: Vec<Message> = message_queue.into();

        assert_eq!(remaining_messages.len(), 1);
        assert_eq!(remaining_messages[0].dest(), ProgramId::from(2));
    }

    #[test]
    /// Test that log works correctly.
    fn log_interaction() {
        // Initialization of InMemoryStorage.
        let mut storage: InMemoryStorage = InMemoryStorage::default();

        // Сhecking that log is empty.
        assert!(storage.log.get().is_empty());

        let message = Message::new_system(0.into(), ProgramId::from(1), vec![1].into(), 128, 512);

        storage.log.put(message.clone());

        assert_eq!(storage.log.get(), [message])
    }
}
