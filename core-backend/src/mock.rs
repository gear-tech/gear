// This file is part of Gear.

// Copyright (C) 2022-2024 Gear Technologies Inc.
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
use alloc::{collections::BTreeSet, rc::Rc, vec, vec::Vec};
use codec::{Decode, Encode};
use core::{fmt, fmt::Debug};
use gear_core::{
    costs::CostToken,
    env::{Externalities, PayloadSliceLock, UnlockPayloadBound},
    env_vars::{EnvVars, EnvVarsV1},
    gas::{ChargeError, CounterType, CountersOwner, GasAmount, GasCounter, GasLeft},
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, MemoryInterval},
    message::{HandlePacket, InitPacket, ReplyPacket},
    pages::{PageNumber, PageU32Size, WasmPage, WASM_PAGE_SIZE},
};
use gear_core_errors::{ReplyCode, SignalCode};
use gear_lazy_pages_common::ProcessAccessError;
use gear_sandbox::{default_executor::Store, AsContextExt, SandboxMemory};
use gear_wasm_instrument::syscalls::SyscallName;
use std::{cell::RefCell, mem};

thread_local! {
    static MEMORY_ACCESSES: RefCell<PreProcessMemoryAccesses> = const { RefCell::new(PreProcessMemoryAccesses::new()) };
}

#[derive(Debug)]
pub struct PreProcessMemoryAccesses {
    pub(crate) reads: Vec<MemoryInterval>,
    pub(crate) writes: Vec<MemoryInterval>,
}

impl PreProcessMemoryAccesses {
    const fn new() -> Self {
        PreProcessMemoryAccesses {
            reads: Vec::new(),
            writes: Vec::new(),
        }
    }

    fn with(f: impl FnOnce(&mut Self)) {
        MEMORY_ACCESSES.with_borrow_mut(f);
    }

    pub fn take() -> Self {
        MEMORY_ACCESSES.with_borrow_mut(|accesses| mem::replace(accesses, Self::new()))
    }
}

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
pub struct MockExt {
    gas_counter: u64,
    forbidden_funcs: BTreeSet<SyscallName>,
}

impl CountersOwner for MockExt {
    fn charge_gas_for_token(&mut self, _token: CostToken) -> Result<(), ChargeError> {
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

    fn decrease_current_counter_to(&mut self, amount: u64) {
        self.gas_counter = amount;
    }

    fn define_current_counter(&mut self) -> u64 {
        self.gas_counter
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
    fn free_range(&mut self, _start: WasmPage, _end: WasmPage) -> Result<(), Self::AllocError> {
        Err(Error)
    }
    fn env_vars(&self, version: u32) -> Result<EnvVars, Self::UnrecoverableError> {
        match version {
            1 => Ok(EnvVars::V1(EnvVarsV1 {
                performance_multiplier: gsys::Percent::new(100),
                existential_deposit: 10,
                mailbox_threshold: 20,
                gas_multiplier: gsys::GasMultiplier::from_value_per_gas(30),
            })),
            _ => unreachable!("Unexpected version of environment variables"),
        }
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
    fn forbidden_funcs(&self) -> &BTreeSet<SyscallName> {
        &self.forbidden_funcs
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
        new_reads: &[MemoryInterval],
        new_writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        PreProcessMemoryAccesses::with(|accesses| {
            accesses.reads.extend(new_reads);
            accesses.writes.extend(new_writes);
        });

        Ok(())
    }
}

#[derive(Debug)]
struct InnerMockMemory {
    pages: Vec<u8>,
    read_attempt_count: u32,
    write_attempt_count: u32,
}

impl InnerMockMemory {
    fn grow(&mut self, new_pages: u32) -> u32 {
        let current_pages = self.pages.len() / WASM_PAGE_SIZE;
        let new_size = self.pages.len() + (new_pages as usize) * WASM_PAGE_SIZE;

        self.pages.resize(new_size, 0);

        current_pages as u32
    }

    fn page_index(&self, offset: u32) -> Option<usize> {
        let offset = offset as usize;

        (offset < self.pages.len()).then_some(offset / WASM_PAGE_SIZE)
    }

    fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), Error> {
        self.write_attempt_count += 1;

        let page_index = self.page_index(offset).ok_or(Error)?;
        let page_offset = offset as usize % WASM_PAGE_SIZE;

        if page_offset + buffer.len() > WASM_PAGE_SIZE {
            return Err(Error);
        }

        let page_start = page_index * WASM_PAGE_SIZE;
        let start = page_start + page_offset;

        if start + buffer.len() > self.pages.len() {
            return Err(Error);
        }

        let dest = &mut self.pages[start..(start + buffer.len())];
        dest.copy_from_slice(buffer);

        Ok(())
    }

    fn read(&mut self, offset: u32, buffer: &mut [u8]) -> Result<(), Error> {
        self.read_attempt_count += 1;

        let page_index = self.page_index(offset).ok_or(Error)?;
        let page_offset = offset as usize % WASM_PAGE_SIZE;

        if page_offset + buffer.len() > WASM_PAGE_SIZE {
            return Err(Error);
        }

        let page_start = page_index * WASM_PAGE_SIZE;
        let start = page_start + page_offset;

        if start + buffer.len() > self.pages.len() {
            return Err(Error);
        }

        let src = &self.pages[start..(start + buffer.len())];
        buffer.copy_from_slice(src);

        Ok(())
    }

    fn size(&self) -> WasmPage {
        WasmPage::new((self.pages.len() / WASM_PAGE_SIZE) as u32).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct MockMemory(Rc<RefCell<InnerMockMemory>>);

impl MockMemory {
    pub fn new(initial_pages: u32) -> Self {
        let size = initial_pages as usize * WASM_PAGE_SIZE;
        let pages = vec![0; size];

        Self(Rc::new(RefCell::new(InnerMockMemory {
            pages,
            read_attempt_count: 0,
            write_attempt_count: 0,
        })))
    }

    pub fn read_attempt_count(&self) -> u32 {
        self.0.borrow().read_attempt_count
    }

    pub fn write_attempt_count(&self) -> u32 {
        self.0.borrow().write_attempt_count
    }

    pub fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), Error> {
        self.0.borrow_mut().write(offset, buffer)
    }
}

impl<T> SandboxMemory<T> for MockMemory {
    fn new(
        _store: &mut Store<T>,
        _initial: u32,
        _maximum: Option<u32>,
    ) -> Result<Self, gear_sandbox::Error> {
        unimplemented!()
    }

    fn read<Context>(
        &self,
        _ctx: &Context,
        ptr: u32,
        buf: &mut [u8],
    ) -> Result<(), gear_sandbox::Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.0
            .borrow_mut()
            .read(ptr, buf)
            .map_err(|_| gear_sandbox::Error::OutOfBounds)
    }

    fn write<Context>(
        &self,
        _ctx: &mut Context,
        ptr: u32,
        value: &[u8],
    ) -> Result<(), gear_sandbox::Error>
    where
        Context: AsContextExt<State = T>,
    {
        self.0
            .borrow_mut()
            .write(ptr, value)
            .map_err(|_| gear_sandbox::Error::OutOfBounds)
    }

    fn grow<Context>(&self, _ctx: &mut Context, new_pages: u32) -> Result<u32, gear_sandbox::Error>
    where
        Context: AsContextExt<State = T>,
    {
        Ok(self.0.borrow_mut().grow(new_pages))
    }

    fn size<Context>(&self, _ctx: &Context) -> u32
    where
        Context: AsContextExt<State = T>,
    {
        self.0.borrow_mut().size().raw()
    }

    unsafe fn get_buff<Context>(&self, _ctx: &mut Context) -> u64
    where
        Context: AsContextExt<State = T>,
    {
        unimplemented!()
    }
}
