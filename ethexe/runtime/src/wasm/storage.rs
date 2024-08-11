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
    state::{Allocations, MemoryPages, MessageQueue, ProgramState, Storage, Waitlist},
    RuntimeInterface,
};
use gear_core::{
    memory::PageBuf, message::Payload, pages::GearPage, reservation::GasReservationMap,
};
use gear_lazy_pages_interface::{LazyPagesInterface, LazyPagesRuntimeInterface};
use gprimitives::H256;

#[derive(Debug, Clone)]
pub struct RuntimeInterfaceStorage;

impl Storage for RuntimeInterfaceStorage {
    fn read_allocations(&self, hash: H256) -> Option<Allocations> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_gas_reservation_map(&self, hash: H256) -> Option<GasReservationMap> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_page_data(&self, hash: H256) -> Option<PageBuf> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_pages(&self, hash: H256) -> Option<MemoryPages> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_payload(&self, hash: H256) -> Option<Payload> {
        // TODO: review this.
        database_ri::read_raw(&hash).map(|slice| slice.to_vec().try_into().unwrap())
    }

    fn read_queue(&self, hash: H256) -> Option<MessageQueue> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_state(&self, hash: H256) -> Option<ProgramState> {
        database_ri::read_unwrapping(&hash)
    }

    fn read_waitlist(&self, hash: H256) -> Option<Waitlist> {
        database_ri::read_unwrapping(&hash)
    }

    fn write_allocations(&self, allocations: Allocations) -> H256 {
        database_ri::write(allocations)
    }

    fn write_gas_reservation_map(&self, gas_reservation_map: GasReservationMap) -> H256 {
        database_ri::write(gas_reservation_map)
    }

    fn write_page_data(&self, data: PageBuf) -> H256 {
        database_ri::write(data)
    }

    fn write_pages(&self, pages: MemoryPages) -> H256 {
        database_ri::write(pages)
    }

    fn write_payload(&self, payload: Payload) -> H256 {
        database_ri::write(payload)
    }

    fn write_queue(&self, queue: MessageQueue) -> H256 {
        database_ri::write(queue)
    }

    fn write_state(&self, state: ProgramState) -> H256 {
        database_ri::write(state)
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> H256 {
        database_ri::write(waitlist)
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

    fn init_lazy_pages(&self, _: BTreeMap<GearPage, H256>) {
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
