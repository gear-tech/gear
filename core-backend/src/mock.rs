// This file is part of Gear.

// Copyright (C) 2022-2025 Gear Technologies Inc.
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
use alloc::{collections::BTreeSet, vec::Vec};
use core::{fmt, fmt::Debug, mem};
use gear_core::{
    costs::CostToken,
    env::{Externalities, PayloadSliceLock, UnlockPayloadBound},
    env_vars::{EnvVars, EnvVarsV1},
    gas::{ChargeError, CounterType, CountersOwner, GasAmount, GasCounter, GasLeft},
    ids::{ActorId, MessageId, ReservationId},
    memory::{Memory, MemoryInterval},
    message::{HandlePacket, InitPacket, MessageContext, ReplyPacket},
    pages::WasmPage,
};
use gear_core_errors::{ReplyCode, SignalCode};
use gear_lazy_pages_common::ProcessAccessError;
use gear_wasm_instrument::syscalls::SyscallName;
use parity_scale_codec::{Decode, Encode};

/// Mock error
#[derive(Debug, Clone, Encode, Decode)]
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
    reads: Vec<MemoryInterval>,
    writes: Vec<MemoryInterval>,
    _forbidden_funcs: BTreeSet<SyscallName>,
}

impl MockExt {
    pub fn take_pre_process_accesses(&mut self) -> (Vec<MemoryInterval>, Vec<MemoryInterval>) {
        (mem::take(&mut self.reads), mem::take(&mut self.writes))
    }
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

    fn decrease_current_counter_to(&mut self, _amount: u64) {}

    fn define_current_counter(&mut self) -> u64 {
        0
    }
}

impl Externalities for MockExt {
    type UnrecoverableError = Error;
    type FallibleError = Error;
    type AllocError = Error;

    fn alloc<Context>(
        &mut self,
        _ctx: &mut Context,
        _mem: &mut impl Memory<Context>,
        _pages_num: u32,
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
                gas_multiplier: gsys::GasMultiplier::from_value_per_gas(100),
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
    fn source(&self) -> Result<ActorId, Self::UnrecoverableError> {
        Ok(ActorId::from(0))
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
    fn program_id(&self) -> Result<ActorId, Self::UnrecoverableError> {
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
    ) -> Result<(MessageId, ActorId), Self::UnrecoverableError> {
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
        &self._forbidden_funcs
    }
    fn msg_ctx(&self) -> &MessageContext {
        unimplemented!()
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
        &mut self,
        new_reads: &[MemoryInterval],
        new_writes: &[MemoryInterval],
        _gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        self.reads.extend(new_reads);
        self.writes.extend(new_writes);

        Ok(())
    }
}

#[cfg(feature = "std")]
pub use with_std_feature::*;

#[cfg(feature = "std")]
mod with_std_feature {
    use gear_core::{
        memory::{HostPointer, Memory, MemoryError},
        pages::{WasmPage, WasmPagesAmount},
    };
    use std::sync::{Arc, Mutex, MutexGuard};

    #[derive(Debug)]
    struct InnerMockMemory {
        pages: Vec<u8>,
        read_attempt_count: u32,
        write_attempt_count: u32,
    }

    impl InnerMockMemory {
        fn grow(&mut self, pages: WasmPagesAmount) -> u32 {
            let size = self.pages.len() as u32;
            let new_size = size + pages.offset() as u32;
            self.pages.resize(new_size as usize, 0);

            size / WasmPage::SIZE
        }

        fn write(&mut self, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
            self.write_attempt_count += 1;

            let offset = offset as usize;
            if offset + buffer.len() > self.pages.len() {
                return Err(MemoryError::AccessOutOfBounds);
            }

            self.pages[offset..offset + buffer.len()].copy_from_slice(buffer);

            Ok(())
        }

        fn read(&mut self, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
            self.read_attempt_count += 1;

            let offset = offset as usize;
            if offset + buffer.len() > self.pages.len() {
                return Err(MemoryError::AccessOutOfBounds);
            }

            buffer.copy_from_slice(&self.pages[offset..(offset + buffer.len())]);

            Ok(())
        }

        fn size(&self) -> WasmPagesAmount {
            WasmPage::from_offset(self.pages.len() as u32).into()
        }
    }

    #[derive(Debug, Clone)]
    pub struct MockMemory(Arc<Mutex<InnerMockMemory>>);

    impl MockMemory {
        pub fn new(initial_pages: u32) -> Self {
            let pages = vec![0; initial_pages as usize * WasmPage::SIZE as usize];

            Self(Arc::new(Mutex::new(InnerMockMemory {
                pages,
                read_attempt_count: 0,
                write_attempt_count: 0,
            })))
        }

        fn lock(&self) -> MutexGuard<'_, InnerMockMemory> {
            self.0.lock().unwrap()
        }

        pub fn read_attempt_count(&self) -> u32 {
            self.lock().read_attempt_count
        }

        pub fn write_attempt_count(&self) -> u32 {
            self.lock().write_attempt_count
        }
    }

    impl<Context> Memory<Context> for MockMemory {
        type GrowError = &'static str;

        fn grow(&self, _ctx: &mut Context, pages: WasmPagesAmount) -> Result<(), Self::GrowError> {
            let _ = self.lock().grow(pages);
            Ok(())
        }

        fn size(&self, _ctx: &Context) -> WasmPagesAmount {
            self.lock().size()
        }

        fn write(&self, _ctx: &mut Context, offset: u32, buffer: &[u8]) -> Result<(), MemoryError> {
            self.lock().write(offset, buffer)
        }

        fn read(&self, _ctx: &Context, offset: u32, buffer: &mut [u8]) -> Result<(), MemoryError> {
            self.lock().read(offset, buffer)
        }

        unsafe fn get_buffer_host_addr_unsafe(&self, _ctx: &Context) -> HostPointer {
            unimplemented!()
        }
    }
}
