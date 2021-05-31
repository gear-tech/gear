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

use gear_core::{
    storage::{AllocationStorage, ProgramStorage, MessageQueue},
    program::{ProgramId, Program},
    memory::PageNumber,
    message::Message,
};

use codec::Encode;
use sp_core::H256;

pub struct ExtAllocationStorage;

pub struct ExtProgramStorage;

#[derive(Default)]
pub struct ExtMessageQueue {
    pub log: Vec<Message>,
}

impl AllocationStorage for ExtAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        gear_common::native::page_info(id.raw())
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        gear_common::native::dealloc(id.raw());
        None
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        gear_common::native::alloc(page.raw(), program)
    }
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
        if message.dest == 0.into() {
            self.log.push(message);
            return;
        }

        let nonce = gear_common::nonce_fetch_inc();

        let message_id = [message.payload.clone().into_raw().encode(), nonce.to_le_bytes().to_vec()].concat();
        let message_id: H256 = sp_io::hashing::blake2_256(&message_id).into();

        gear_common::native::queue_message(message, message_id)
    }
}
