// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::interface::database_ri;
use alloc::{collections::BTreeMap, vec::Vec};
use core_processor::configs::BlockInfo;
use ethexe_runtime_common::{
    state::{
        Allocations, DispatchStash, HashOf, Mailbox, MemoryPages, MessageQueue, ProgramState,
        Storage, Waitlist,
    },
    RuntimeInterface,
};
use gear_core::{memory::PageBuf, message::Payload, pages::GearPage};
use gear_lazy_pages_interface::{LazyPagesInterface, LazyPagesRuntimeInterface};
use gprimitives::H256;

#[derive(Debug, Clone)]
pub struct RuntimeInterfaceStorage;

impl Storage for RuntimeInterfaceStorage {
    fn read_state(&self, hash: H256) -> Option<ProgramState> {
        database_ri::read_unwrapping(&hash)
    }

    fn write_state(&self, state: ProgramState) -> H256 {
        if state.is_zero() {
            return H256::zero();
        }

        database_ri::write(state)
    }

    fn read_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(database_ri::write(queue)) }
    }

    fn read_waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(database_ri::write(waitlist)) }
    }

    fn read_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(database_ri::write(stash)) }
    }

    fn read_mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(database_ri::write(mailbox)) }
    }

    fn read_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(database_ri::write(pages)) }
    }

    fn read_allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(database_ri::write(allocations)) }
    }

    fn read_payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        // TODO: review this.
        database_ri::read_raw(&hash.hash()).map(|slice| slice.to_vec().try_into().unwrap())
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(database_ri::write(payload)) }
    }

    fn read_page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        database_ri::read_unwrapping(&hash.hash())
    }

    fn write_page_data(&self, data: PageBuf) -> HashOf<PageBuf> {
        unsafe { HashOf::new(database_ri::write(data)) }
    }
}

#[derive(Debug, Clone)]
pub struct NativeRuntimeInterface {
    pub(crate) block_info: BlockInfo,
    pub(crate) storage: RuntimeInterfaceStorage,
}

impl RuntimeInterface<RuntimeInterfaceStorage> for NativeRuntimeInterface {
    type LazyPages = LazyPagesRuntimeInterface;

    fn block_info(&self) -> BlockInfo {
        self.block_info
    }

    fn init_lazy_pages(&self, _: BTreeMap<GearPage, HashOf<PageBuf>>) {
        assert!(Self::LazyPages::try_to_enable_lazy_pages(Default::default()))
    }

    fn random_data(&self) -> (Vec<u8>, u32) {
        // TODO: set real value
        Default::default()
    }

    fn storage(&self) -> &RuntimeInterfaceStorage {
        &self.storage
    }
}
