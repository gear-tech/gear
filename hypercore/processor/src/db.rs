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

//! Processor storage implementation.

use gear_core::{
    code::InstrumentedCode,
    ids::{CodeId, ProgramId},
    memory::PageBuf,
    message::Payload,
    reservation::GasReservationMap,
};
use hypercore_db::{CASDatabase, KVDatabase};
use hypercore_runtime_common::state::{
    Allocations, MemoryPages, MessageQueue, ProgramState, Waitlist,
};
use parity_scale_codec::{Decode, Encode};
use primitive_types::H256;

pub use hypercore_runtime_common::state::Storage;

pub struct Database {
    cas: Box<dyn CASDatabase>,
    kv: Box<dyn KVDatabase>,
}

impl Database {
    pub fn new(cas: Box<dyn CASDatabase>, kv: Box<dyn KVDatabase>) -> Self {
        Self { cas, kv }
    }

    pub fn from_one<DB: CASDatabase + KVDatabase>(db: &DB) -> Self {
        Self {
            cas: CASDatabase::clone_boxed(db),
            kv: KVDatabase::clone_boxed_kv(db),
        }
    }

    pub fn get_program_code_id(&self, program_id: ProgramId) -> Option<CodeId> {
        let key = [
            "program_to_code_id".as_bytes(),
            program_id.into_bytes().as_slice(),
        ]
        .concat();
        let data = self.kv.get(&key)?;
        Some(CodeId::try_from(data.as_slice()).expect("Failed to decode data into `CodeId`"))
    }

    pub fn set_program_code_id(&self, program_id: ProgramId, code_id: CodeId) {
        let key = [
            "program_to_code_id".as_bytes(),
            program_id.into_bytes().as_slice(),
        ]
        .concat();
        self.kv.put(&key, code_id.into_bytes().to_vec());
    }

    pub fn read_original_code(&self, code_id: CodeId) -> Option<Vec<u8>> {
        let hash = H256::from(code_id.into_bytes());
        self.cas.read(&hash)
    }

    pub fn write_original_code(&self, code: &[u8]) -> H256 {
        self.cas.write(code)
    }

    pub fn read_instrumented_code(
        &self,
        runtime_id: u32,
        code_id: CodeId,
    ) -> Option<InstrumentedCode> {
        let key = [
            "instrumented_code".as_bytes(),
            runtime_id.to_be_bytes().as_slice(),
            code_id.into_bytes().as_slice(),
        ]
        .concat();
        let data = self.kv.get(&key)?;
        Some(
            InstrumentedCode::decode(&mut data.as_slice())
                .expect("Failed to decode data into `InstrumentedCode`"),
        )
    }

    pub fn write_instrumented_code(
        &self,
        runtime_id: u32,
        code_id: CodeId,
        code: InstrumentedCode,
    ) {
        let key = [
            "instrumented_code".as_bytes(),
            runtime_id.to_be_bytes().as_slice(),
            code_id.into_bytes().as_slice(),
        ]
        .concat();
        self.kv.put(&key, code.encode());
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self {
            cas: self.cas.clone_boxed(),
            kv: self.kv.clone_boxed_kv(),
        }
    }
}

// TODO: consider to change decode panics to Results.
impl Storage for Database {
    fn read_state(&self, hash: H256) -> Option<ProgramState> {
        let data = self.cas.read(&hash)?;
        Some(
            ProgramState::decode(&mut &data[..])
                .expect("Failed to decode data into `ProgramState`"),
        )
    }

    fn write_state(&self, state: ProgramState) -> H256 {
        self.cas.write(&state.encode())
    }

    fn read_queue(&self, hash: H256) -> Option<MessageQueue> {
        let data = self.cas.read(&hash)?;
        Some(
            MessageQueue::decode(&mut &data[..])
                .expect("Failed to decode data into `MessageQueue`"),
        )
    }

    fn write_queue(&self, queue: MessageQueue) -> H256 {
        self.cas.write(&queue.encode())
    }

    fn read_waitlist(&self, hash: H256) -> Option<Waitlist> {
        self.cas.read(&hash).map(|data| {
            Waitlist::decode(&mut data.as_slice()).expect("Failed to decode data into `Waitlist`")
        })
    }

    fn write_waitlist(&self, waitlist: Waitlist) -> H256 {
        self.cas.write(&waitlist.encode())
    }

    fn read_pages(&self, hash: H256) -> Option<MemoryPages> {
        let data = self.cas.read(&hash)?;
        Some(MemoryPages::decode(&mut &data[..]).expect("Failed to decode data into `MemoryPages`"))
    }

    fn write_pages(&self, pages: MemoryPages) -> H256 {
        self.cas.write(&pages.encode())
    }

    fn read_allocations(&self, hash: H256) -> Option<Allocations> {
        let data = self.cas.read(&hash)?;
        Some(Allocations::decode(&mut &data[..]).expect("Failed to decode data into `Allocations`"))
    }

    fn write_allocations(&self, allocations: Allocations) -> H256 {
        self.cas.write(&allocations.encode())
    }

    fn read_gas_reservation_map(&self, hash: H256) -> Option<GasReservationMap> {
        let data = self.cas.read(&hash)?;
        Some(
            GasReservationMap::decode(&mut &data[..])
                .expect("Failed to decode data into `GasReservationMap`"),
        )
    }

    fn write_gas_reservation_map(&self, gas_reservation_map: GasReservationMap) -> H256 {
        self.cas.write(&gas_reservation_map.encode())
    }

    fn read_payload(&self, hash: H256) -> Option<Payload> {
        let data = self.cas.read(&hash)?;
        Some(Payload::try_from(data).expect("Failed to decode data into `Payload`"))
    }

    fn write_payload(&self, payload: Payload) -> H256 {
        self.cas.write(payload.inner())
    }

    fn read_page_data(&self, hash: H256) -> Option<PageBuf> {
        let data = self.cas.read(&hash)?;
        Some(PageBuf::decode(&mut data.as_slice()).expect("Failed to decode data into `PageBuf`"))
    }

    fn write_page_data(&self, data: PageBuf) -> H256 {
        self.cas.write(&data)
    }
}
