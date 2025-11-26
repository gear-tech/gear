// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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
use alloc::vec::Vec;
use core_processor::configs::BlockInfo;
use ethexe_common::HashOf;
use ethexe_runtime_common::{
    RuntimeInterface,
    state::{
        Allocations, DispatchStash, Mailbox, MemoryPages, MemoryPagesRegion, MessageQueue,
        ProgramState, Storage, UserMailbox, Waitlist,
    },
};
use gear_core::{buffer::Payload, memory::PageBuf};
use gear_lazy_pages_interface::{LazyPagesInterface, LazyPagesRuntimeInterface};
use gprimitives::H256;

#[derive(Debug, Clone)]
pub struct RuntimeInterfaceStorage;

impl Storage for RuntimeInterfaceStorage {
    fn program_state(&self, hash: H256) -> Option<ProgramState> {
        if hash.is_zero() {
            Some(ProgramState::zero())
        } else {
            database_ri::read_unwrapping(&hash)
        }
    }

    fn write_program_state(&self, state: ProgramState) -> H256 {
        if state.is_zero() {
            return H256::zero();
        }

        database_ri::write(state)
    }

    fn message_queue(&self, hash: HashOf<MessageQueue>) -> Option<MessageQueue> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_message_queue(&self, queue: MessageQueue) -> HashOf<MessageQueue> {
        unsafe { HashOf::new(database_ri::write(queue)) }
    }

    fn waitlist(&self, hash: HashOf<Waitlist>) -> Option<Waitlist> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> HashOf<Waitlist> {
        unsafe { HashOf::new(database_ri::write(waitlist)) }
    }

    fn dispatch_stash(&self, hash: HashOf<DispatchStash>) -> Option<DispatchStash> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_dispatch_stash(&self, stash: DispatchStash) -> HashOf<DispatchStash> {
        unsafe { HashOf::new(database_ri::write(stash)) }
    }

    fn mailbox(&self, hash: HashOf<Mailbox>) -> Option<Mailbox> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_mailbox(&self, mailbox: Mailbox) -> HashOf<Mailbox> {
        unsafe { HashOf::new(database_ri::write(mailbox)) }
    }

    fn user_mailbox(&self, hash: HashOf<UserMailbox>) -> Option<UserMailbox> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_user_mailbox(&self, user_mailbox: UserMailbox) -> HashOf<UserMailbox> {
        unsafe { HashOf::new(database_ri::write(user_mailbox)) }
    }

    fn memory_pages(&self, hash: HashOf<MemoryPages>) -> Option<MemoryPages> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn memory_pages_region(&self, hash: HashOf<MemoryPagesRegion>) -> Option<MemoryPagesRegion> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_memory_pages(&self, pages: MemoryPages) -> HashOf<MemoryPages> {
        unsafe { HashOf::new(database_ri::write(pages)) }
    }

    fn write_memory_pages_region(
        &self,
        pages_region: MemoryPagesRegion,
    ) -> HashOf<MemoryPagesRegion> {
        unsafe { HashOf::new(database_ri::write(pages_region)) }
    }

    fn allocations(&self, hash: HashOf<Allocations>) -> Option<Allocations> {
        database_ri::read_unwrapping(&hash.inner())
    }

    fn write_allocations(&self, allocations: Allocations) -> HashOf<Allocations> {
        unsafe { HashOf::new(database_ri::write(allocations)) }
    }

    fn payload(&self, hash: HashOf<Payload>) -> Option<Payload> {
        // TODO: review this.
        database_ri::read_raw(&hash.inner()).map(|slice| slice.to_vec().try_into().unwrap())
    }

    fn write_payload(&self, payload: Payload) -> HashOf<Payload> {
        unsafe { HashOf::new(database_ri::write(payload)) }
    }

    fn page_data(&self, hash: HashOf<PageBuf>) -> Option<PageBuf> {
        database_ri::read_unwrapping(&hash.inner())
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

    fn init_lazy_pages(&self) {
        assert!(Self::LazyPages::try_to_enable_lazy_pages(Default::default()))
    }

    fn random_data(&self) -> (Vec<u8>, u32) {
        // TODO: set real value
        Default::default()
    }

    fn storage(&self) -> &RuntimeInterfaceStorage {
        &self.storage
    }

    fn update_state_hash(&self, hash: &H256) {
        database_ri::update_state_hash(hash);
    }
}
