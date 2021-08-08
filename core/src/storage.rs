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

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use hashbrown::HashMap;

use crate::{
    message::Message,
    program::{Program, ProgramId},
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
        Self {
            inner: programs.into_iter().map(|p| (p.id(), p)).collect(),
        }
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
        messages.into_iter().for_each(|m| self.queue(m));
    }
}

/// In-memory message queue (for tests).
pub struct InMemoryMessageQueue {
    inner: VecDeque<Message>,
    log: Vec<Message>, // messages sent to /0
}

impl InMemoryMessageQueue {
    /// New in-memory message queue consisting of the provided messages.
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            inner: VecDeque::from(messages),
            log: Vec::new(),
        }
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

/// Storage.
pub struct Storage<MQ: MessageQueue, PS: ProgramStorage> {
    /// Message queue stoage.
    pub message_queue: MQ,
    /// Program storage.
    pub program_storage: PS,
}

/// Fully in-memory storage (for tests).
pub type InMemoryStorage = Storage<InMemoryMessageQueue, InMemoryProgramStorage>;

/// Create new in-memory storage for tests by providing all data.
pub fn new_in_memory(messages: Vec<Message>, programs: Vec<Program>) -> InMemoryStorage {
    Storage {
        message_queue: InMemoryMessageQueue::new(messages),
        program_storage: InMemoryProgramStorage::new(programs),
    }
}

/// Create new empty in-memory storage for tests.
pub fn new_in_memory_empty() -> InMemoryStorage {
    Storage {
        message_queue: InMemoryMessageQueue::new(Default::default()),
        program_storage: InMemoryProgramStorage::new(Default::default()),
    }
}

#[cfg(test)]
/// This module contains tests of parts of InMemoryStorage:
/// of allocation storage, message queue storage and program storage
mod tests {
    extern crate wabt;
    use super::*;

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
        let mut program_storage = InMemoryProgramStorage::new(vec![
            Program::new(id1, binary.clone(), Default::default()).expect("err create program"),
            Program::new(id2, binary.clone(), Default::default()).expect("err create program"),
        ]);

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
        let remaining_programs = program_storage.drain();
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
        let mut message_queue = InMemoryMessageQueue::new(vec![]);

        // Сhecking that the storage totally empty
        assert!(message_queue.dequeue().is_none());
        assert!(message_queue.log().is_empty());

        // Addition of new system message
        message_queue.queue(Message::new_system(
            0.into(),
            ProgramId::system(),
            Payload::from(vec![0]),
            128,
            256,
        ));

        // Сhecking that the system message gets in logs
        assert!(!message_queue.log().is_empty());
        assert_eq!(message_queue.log()[0].value(), 256u128);

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
        let remaining_messages = message_queue.drain();

        assert_eq!(remaining_messages.len(), 1);
        assert_eq!(remaining_messages[0].dest(), ProgramId::from(2));
    }
}
