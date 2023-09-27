// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

use crate::{
    error::{
        BackendAllocSyscallError, BackendSyscallError, RunFallibleError, UndefinedTerminationReason,
    },
    BackendExternalities,
};
use alloc::{collections::BTreeSet, vec, vec::Vec};
use codec::{Decode, Encode};
use core::{cell::Cell, fmt, fmt::Debug};
use gear_core::{
    costs::RuntimeCosts,
    env::{Externalities, PayloadSliceLock, UnlockPayloadBound},
    gas::{ChargeError, CounterType, CountersOwner, GasAmount, GasCounter, GasLeft},
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, MemoryError, MemoryInterval},
    message::{HandlePacket, InitPacket, ReplyPacket},
    pages::{PageNumber, PageU32Size, WasmPage, WASM_PAGE_SIZE},
};
use gear_core_errors::{ReplyCode, SignalCode};
use gear_lazy_pages_common::ProcessAccessError;
use gear_wasm_instrument::syscalls::SysCallName;

/// Mock error
#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = codec)]
pub struct Error;

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!()
    }
}

impl BackendSyscallError for Error {
    fn into_termination_reason(self) -> UndefinedTerminationReason {
        unimplemented!()
    }

    fn into_run_fallible_error(self) -> RunFallibleError {
        unimplemented!()
    }
}

impl BackendAllocSyscallError for Error {
    type ExtError = Self;

    fn into_backend_error(self) -> Result<Self::ExtError, Self> {
        unimplemented!()
    }
}

/// Mock ext
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct MockExt(BTreeSet<SysCallName>);

impl CountersOwner for MockExt {
    fn charge_gas_runtime(&mut self, _cost: RuntimeCosts) -> Result<(), ChargeError> {
        Ok(())
    }

    fn charge_gas_runtime_if_enough(&mut self, _cost: RuntimeCosts) -> Result<(), ChargeError> {
        Ok(())
    }

    fn charge_gas_if_enough(&mut self, _amount: u64) -> Result<(), ChargeError> {
        Ok(())
    }

    fn gas_left(&self) -> GasLeft {
        (0u64, 0u64).into()
    }

    fn current_counter_type(&self) -> CounterType {
        CounterType::GasLimit
    }

    fn decrease_current_counter_to(&mut self, _amount: u64) {}

    fn define_current_counter(&mut self) -> u64 {
        0
    }
}

impl Externalities for MockExt {
    type UnrecoverableError = Error;
    type FallibleError = Error;
    type AllocError = Error;

    fn alloc(
        &mut self,
        _pages_num: u32,
        _mem: &mut impl Memory,
    ) -> Result<WasmPage, Self::AllocError> {
        Err(Error)
    }
    fn free(&mut self, _page: WasmPage) -> Result<(), Self::AllocError> {
        Err(Error)
    }
    fn block_height(&self) -> Result<u32, Self::UnrecoverableError> {
        Ok(0)
    }
    fn block_timestamp(&self) -> Result<u64, Self::UnrecoverableError> {
        Ok(0)
    }
    fn send_init(&mut self) -> Result<u32, Self::UnrecoverableError> {
        Ok(0)
    }
    fn send_push(&mut self, _handle: u32, _buffer: &[u8]) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn reply_commit(&mut self, _msg: ReplyPacket) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(MessageId::default())
    }
    fn send_push_input(
        &mut self,
        _handle: u32,
        _offset: u32,
        _len: u32,
    ) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn reply_push(&mut self, _buffer: &[u8]) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn send_commit(
        &mut self,
        _handle: u32,
        _msg: HandlePacket,
        _delay: u32,
    ) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(MessageId::default())
    }
    fn reply_to(&self) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(Default::default())
    }
    fn reply_push_input(
        &mut self,
        _offset: u32,
        _len: u32,
    ) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn source(&self) -> Result<ProgramId, Self::UnrecoverableError> {
        Ok(ProgramId::from(0))
    }
    fn reply_code(&self) -> Result<ReplyCode, Self::UnrecoverableError> {
        Ok(Default::default())
    }
    fn signal_code(&self) -> Result<SignalCode, Self::UnrecoverableError> {
        Ok(Default::default())
    }
    fn message_id(&self) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(0.into())
    }
    fn pay_program_rent(
        &mut self,
        _program_id: ProgramId,
        _rent: u128,
    ) -> Result<(u128, u32), Self::UnrecoverableError> {
        Ok((0, 0))
    }
    fn program_id(&self) -> Result<ProgramId, Self::UnrecoverableError> {
        Ok(0.into())
    }
    fn debug(&self, _data: &str) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn size(&self) -> Result<usize, Self::UnrecoverableError> {
        Ok(0)
    }
    fn gas_available(&self) -> Result<u64, Self::UnrecoverableError> {
        Ok(1_000_000)
    }
    fn value(&self) -> Result<u128, Self::UnrecoverableError> {
        Ok(0)
    }
    fn value_available(&self) -> Result<u128, Self::UnrecoverableError> {
        Ok(1_000_000)
    }
    fn random(&self) -> Result<(&[u8], u32), Self::UnrecoverableError> {
        Ok(([0u8; 32].as_ref(), 0))
    }
    fn wait(&mut self) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn wait_for(&mut self, _duration: u32) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn wait_up_to(&mut self, _duration: u32) -> Result<bool, Self::UnrecoverableError> {
        Ok(false)
    }
    fn wake(&mut self, _waker_id: MessageId, _delay: u32) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn create_program(
        &mut self,
        _packet: InitPacket,
        _delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::UnrecoverableError> {
        Ok((Default::default(), Default::default()))
    }
    fn reply_deposit(
        &mut self,
        _message_id: MessageId,
        _amount: u64,
    ) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }
    fn forbidden_funcs(&self) -> &BTreeSet<SysCallName> {
        &self.0
    }
    fn reserve_gas(
        &mut self,
        _amount: u64,
        _duration: u32,
    ) -> Result<ReservationId, Self::UnrecoverableError> {
        Ok(ReservationId::default())
    }
    fn unreserve_gas(&mut self, _id: ReservationId) -> Result<u64, Self::UnrecoverableError> {
        Ok(0)
    }

    fn system_reserve_gas(&mut self, _amount: u64) -> Result<(), Self::UnrecoverableError> {
        Ok(())
    }

    fn reservation_send_commit(
        &mut self,
        _id: ReservationId,
        _handle: u32,
        _msg: HandlePacket,
        _delay: u32,
    ) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(MessageId::default())
    }

    fn reservation_reply_commit(
        &mut self,
        _id: ReservationId,
        _msg: ReplyPacket,
    ) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(MessageId::default())
    }

    fn signal_from(&self) -> Result<MessageId, Self::UnrecoverableError> {
        Ok(MessageId::default())
    }

    fn lock_payload(
        &mut self,
        _at: u32,
        _len: u32,
    ) -> Result<PayloadSliceLock, Self::UnrecoverableError> {
        unimplemented!()
    }

    fn unlock_payload(&mut self, _payload_holder: &mut PayloadSliceLock) -> UnlockPayloadBound {
        unimplemented!()
    }
}

impl BackendExternalities for MockExt {
    fn gas_amount(&self) -> GasAmount {
        GasCounter::new(0).to_amount()
    }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct MockMemory {
    pages: Vec<u8>,
    read_attempt_count: Cell<u32>,
    write_attempt_count: Cell<u32>,
}

impl MockMemory {
    pub fn new(initial_pages: u32) -> Self {
        let size = initial_pages as usize * WASM_PAGE_SIZE;
        let pages = vec![0; size];

        Self {
            pages,
            read_attempt_count: Cell::new(0),
            write_attempt_count: Cell::new(0),
        }
    }

    pub fn read_attempt_count(&self) -> u32 {
        self.read_attempt_count.get()
    }

    pub fn write_attempt_count(&self) -> u32 {
        self.write_attempt_count.get()
    }

    fn page_index(&self, offset: u32) -> Option<usize> {
        let offset = offset as usize;

        (offset < self.pages.len()).then_some(offset / WASM_PAGE_SIZE)
    }
}

impl Memory for MockMemory {
    type GrowError = ();

    fn grow(&mut self, pages: WasmPage) -> Result<(), Self::GrowError> {
        let new_size = self.pages.len() + (pages.raw() as usize) * WASM_PAGE_SIZE;

        self.pages.resize(new_size, 0);

        Ok(())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new((self.pages.len() / WASM_PAGE_SIZE) as u32).unwrap_or_default()
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
        self.write_attempt_count.set(self.write_attempt_count() + 1);
        let page_index = self
            .page_index(offset)
            .ok_or(MemoryError::AccessOutOfBounds)?;
        let page_offset = offset as usize % WASM_PAGE_SIZE;

        if page_offset + buffer.len() > WASM_PAGE_SIZE {
            return Err(MemoryError::AccessOutOfBounds);
        }

        let page_start = page_index * WASM_PAGE_SIZE;
        let start = page_start + page_offset;

        if start + buffer.len() > self.pages.len() {
            return Err(MemoryError::AccessOutOfBounds);
        }

        let dest = &mut self.pages[start..(start + buffer.len())];
        dest.copy_from_slice(buffer);

        Ok(())
    }

    fn read(&self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
        self.read_attempt_count.set(self.read_attempt_count() + 1);
        let page_index = self
            .page_index(offset)
            .ok_or(MemoryError::AccessOutOfBounds)?;
        let page_offset = offset as usize % WASM_PAGE_SIZE;

        if page_offset + buffer.len() > WASM_PAGE_SIZE {
            return Err(MemoryError::AccessOutOfBounds);
        }

        let page_start = page_index * WASM_PAGE_SIZE;
        let start = page_start + page_offset;

        if start + buffer.len() > self.pages.len() {
            return Err(MemoryError::AccessOutOfBounds);
        }

        let src = &self.pages[start..(start + buffer.len())];
        buffer.copy_from_slice(src);

        Ok(())
    }

    unsafe fn get_buffer_host_addr_unsafe(&mut self) -> u64 {
        unimplemented!();
    }
}
