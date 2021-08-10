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

use std::collections::BTreeSet;

use gear_core::{
    message::{Message, MessageId},
    program::{Program, ProgramId},
    storage::{MessageMap, MessageQueue, ProgramStorage, WaitList},
};
#[derive(Default)]
pub struct ExtProgramStorage;

#[derive(Default)]
pub struct ExtMessageQueue {
    pub log: Vec<Message>,
}

#[derive(Default)]
pub struct ExtWaitList {
    cache: BTreeSet<MessageId>,
}

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        gear_common::native::get_program(id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        gear_common::native::set_program(program);
        None
    }

    fn remove(&mut self, _id: ProgramId) -> Option<Program> {
        unimplemented!()
    }
}

impl MessageQueue for ExtMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        gear_common::native::dequeue_message()
    }

    fn queue(&mut self, message: Message) {
        // We queue message only when there is a destination.
        if gear_common::native::program_exists(message.dest) {
            gear_common::native::queue_message(message);
            return;
        }

        // If no destination, message is considered to be a log record.
        self.log.push(message);
    }
}

impl WaitList for ExtWaitList {
    fn insert(&mut self, id: MessageId, message: Message) {
        self.cache.insert(id);
        gear_common::native::insert_waiting_message(id, message);
    }

    fn remove(&mut self, id: MessageId) -> Option<Message> {
        self.cache.remove(&id);
        gear_common::native::remove_waiting_message(id)
    }
}

impl From<ExtWaitList> for MessageMap {
    fn from(queue: ExtWaitList) -> MessageMap {
        let mut map = MessageMap::new();
        queue.cache.into_iter().for_each(|id| {
            if let Some(msg) = gear_common::native::get_waiting_message(id) {
                map.insert(id, msg);
            }
        });
        map
    }
}
