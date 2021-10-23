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

use gear_common::STORAGE_PROGRAM_PREFIX;
use gear_core::{
    message::{Message, MessageId},
    program::{Program, ProgramId},
    storage::{MessageMap, MessageQueue, ProgramStorage, WaitList},
};
use sp_std::collections::btree_set::BTreeSet;
use sp_std::prelude::*;

#[derive(Default)]
pub struct ExtProgramStorage;

#[derive(Default)]
pub struct ExtMessageQueue {
    pub log: Vec<Message>,
}

#[derive(Default)]
pub struct ExtWaitList {
    cache: BTreeSet<(ProgramId, MessageId)>,
}

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        gear_common::native::get_program(id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        gear_common::native::set_program(program);
        None
    }

    fn exists(&self, id: ProgramId) -> bool {
        gear_common::native::program_exists(id)
    }

    fn remove(&mut self, _id: ProgramId) -> Option<Program> {
        unimplemented!()
    }
}

impl ExtProgramStorage {
    pub fn iter(&self) -> ExtProgramStorageIter {
        ExtProgramStorageIter {
            key: Some(STORAGE_PROGRAM_PREFIX.to_vec()),
        }
    }
}

pub struct ExtProgramStorageIter {
    key: Option<Vec<u8>>,
}

impl Iterator for ExtProgramStorageIter {
    type Item = Program;

    fn next(&mut self) -> Option<Self::Item> {
        self.key = self.key.as_ref().and_then(|key| {
            sp_io::storage::next_key(key).filter(|key| key.starts_with(STORAGE_PROGRAM_PREFIX))
        });

        self.key.as_ref().and_then(|key| {
            gear_common::native::get_program(ProgramId::from_slice(
                &key[STORAGE_PROGRAM_PREFIX.len()..],
            ))
        })
    }
}

impl MessageQueue for ExtMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        gear_common::native::dequeue_message()
    }

    fn queue(&mut self, message: Message) {
        gear_common::native::queue_message(message);
    }
}

impl WaitList for ExtWaitList {
    fn insert(&mut self, prog_id: ProgramId, msg_id: MessageId, message: Message) {
        self.cache.insert((prog_id, msg_id));
        gear_common::native::insert_waiting_message(prog_id, msg_id, message);
    }

    fn remove(&mut self, prog_id: ProgramId, msg_id: MessageId) -> Option<Message> {
        self.cache.remove(&(prog_id, msg_id));
        gear_common::native::remove_waiting_message(prog_id, msg_id)
    }
}

impl From<ExtWaitList> for MessageMap {
    fn from(queue: ExtWaitList) -> MessageMap {
        let mut map = MessageMap::new();
        queue.cache.into_iter().for_each(|(prog_id, msg_id)| {
            if let Some(msg) = gear_common::native::get_waiting_message(prog_id, msg_id) {
                map.insert((prog_id, msg_id), msg);
            }
        });
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gear_common::{STORAGE_CODE_PREFIX, STORAGE_MESSAGE_PREFIX, STORAGE_WAITLIST_PREFIX};
    use gear_core::message::Payload;

    fn new_test_ext() -> sp_io::TestExternalities {
        frame_system::GenesisConfig::default()
            .build_storage::<gear_runtime::Runtime>()
            .unwrap()
            .into()
    }

    fn new_test_storage(
    ) -> gear_core::storage::Storage<ExtMessageQueue, ExtProgramStorage, ExtWaitList> {
        sp_io::storage::clear_prefix(STORAGE_CODE_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_MESSAGE_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_PROGRAM_PREFIX, None);
        sp_io::storage::clear_prefix(STORAGE_WAITLIST_PREFIX, None);
        gear_core::storage::Storage {
            message_queue: Default::default(),
            program_storage: ExtProgramStorage,
            wait_list: Default::default(),
            log: Default::default(),
        }
    }

    fn parse_wat(source: &str) -> Vec<u8> {
        wabt::Wat2Wasm::new()
            .validate(false)
            .convert(source)
            .expect("failed to parse module")
            .as_ref()
            .to_vec()
    }

    #[test]
    fn program_storage_iterator() {
        new_test_ext().execute_with(|| {
            let mut storage = new_test_storage();

            let wat = r#"
            (module
                (import "env" "memory" (memory 1))
            )"#;
            let code = parse_wat(wat);

            for id in 1..=10 {
                let program =
                    Program::new(ProgramId::from(id), code.clone(), Default::default()).unwrap();
                storage.program_storage.set(program);
            }

            // Since `sp_io::storage::next_key` iterates in lexicographic order, message is inserted into wait list
            // with prefix `g::wait::` to make sure iterator exhausts correctly.
            let msg_id = MessageId::from(1);
            storage.wait_list.insert(
                ProgramId::from(1),
                msg_id,
                Message::new_system(msg_id, ProgramId::from(1), Payload::from(vec![]), 0, 0),
            );

            let programs_count = storage.program_storage.iter().count();
            assert_eq!(programs_count, 10)
        })
    }
}
