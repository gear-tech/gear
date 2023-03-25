// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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
    memory::ProcessAccessError, BackendAllocExtError, BackendExt, BackendExtError, ExtInfo,
    SystemReservationContext, TerminationReason,
};
use alloc::collections::BTreeSet;
use core::fmt;
use gear_core::{
    costs::RuntimeCosts,
    env::Ext,
    gas::{ChargeError, CountersOwner, GasAmount, GasCounter, GasLeft},
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, MemoryInterval, WasmPage},
    message::{HandlePacket, InitPacket, ReplyPacket, StatusCode},
    reservation::GasReserver,
};
use gear_core_errors::MemoryError;
use gear_wasm_instrument::syscalls::SysCallName;
use scale_info::scale::{self, Decode, Encode};

/// Mock error
#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = scale)]
pub struct Error;

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        unimplemented!()
    }
}

impl BackendExtError for Error {
    fn into_termination_reason(self) -> TerminationReason {
        unimplemented!()
    }
}

impl BackendAllocExtError for Error {
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

    fn refund_gas(&mut self, _amount: u64) -> Result<(), ChargeError> {
        Ok(())
    }

    fn gas_left(&self) -> GasLeft {
        GasLeft {
            gas: 0,
            allowance: 0,
        }
    }

    fn set_gas_left(&mut self, _gas_left: GasLeft) {}
}

impl Ext for MockExt {
    type Error = Error;
    type AllocError = Error;

    fn alloc(
        &mut self,
        _pages: WasmPage,
        _mem: &mut impl Memory,
    ) -> Result<WasmPage, Self::AllocError> {
        Err(Error)
    }
    fn free(&mut self, _page: WasmPage) -> Result<(), Self::AllocError> {
        Err(Error)
    }
    fn block_height(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        Ok(0)
    }
    fn origin(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(ProgramId::from(0))
    }
    fn send_init(&mut self) -> Result<u32, Self::Error> {
        Ok(0)
    }
    fn send_push(&mut self, _handle: u32, _buffer: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reply_commit(&mut self, _msg: ReplyPacket, _delay: u32) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }
    fn send_push_input(
        &mut self,
        _handle: u32,
        _offset: u32,
        _len: u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
    fn reply_push(&mut self, _buffer: &[u8]) -> Result<(), Self::Error> {
        Ok(())
    }
    fn send_commit(
        &mut self,
        _handle: u32,
        _msg: HandlePacket,
        _delay: u32,
    ) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }
    fn reply_to(&mut self) -> Result<MessageId, Self::Error> {
        Ok(Default::default())
    }
    fn reply_push_input(&mut self, _offset: u32, _len: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(ProgramId::from(0))
    }
    fn status_code(&mut self) -> Result<StatusCode, Self::Error> {
        Ok(Default::default())
    }
    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        Ok(0.into())
    }
    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        Ok(0.into())
    }
    fn debug(&mut self, _data: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn read(&mut self, _at: u32, _len: u32) -> Result<(&[u8], GasLeft), Self::Error> {
        Ok((&[], Default::default()))
    }
    fn size(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        Ok(1_000_000)
    }
    fn value(&mut self) -> Result<u128, Self::Error> {
        Ok(0)
    }
    fn value_available(&mut self) -> Result<u128, Self::Error> {
        Ok(1_000_000)
    }
    fn random(&mut self) -> Result<(&[u8], u32), Self::Error> {
        Ok(([0u8; 32].as_ref(), 0))
    }
    fn wait(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn wait_for(&mut self, _duration: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn wait_up_to(&mut self, _duration: u32) -> Result<bool, Self::Error> {
        Ok(false)
    }
    fn wake(&mut self, _waker_id: MessageId, _delay: u32) -> Result<(), Self::Error> {
        Ok(())
    }
    fn create_program(
        &mut self,
        _packet: InitPacket,
        _delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error> {
        Ok((Default::default(), Default::default()))
    }
    fn forbidden_funcs(&self) -> &BTreeSet<SysCallName> {
        &self.0
    }
    fn reserve_gas(&mut self, _amount: u64, _duration: u32) -> Result<ReservationId, Self::Error> {
        Ok(ReservationId::default())
    }
    fn unreserve_gas(&mut self, _id: ReservationId) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn system_reserve_gas(&mut self, _amount: u64) -> Result<(), Self::Error> {
        Ok(())
    }

    fn reservation_send_commit(
        &mut self,
        _id: ReservationId,
        _handle: u32,
        _msg: HandlePacket,
        _delay: u32,
    ) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }

    fn reservation_reply_commit(
        &mut self,
        _id: ReservationId,
        _msg: ReplyPacket,
        _delay: u32,
    ) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }

    fn signal_from(&mut self) -> Result<MessageId, Self::Error> {
        Ok(MessageId::default())
    }
}

impl BackendExt for MockExt {
    fn into_ext_info(self, _memory: &impl Memory) -> Result<ExtInfo, MemoryError> {
        Ok(ExtInfo {
            gas_amount: GasCounter::new(0).to_amount(),
            gas_reserver: GasReserver::new(Default::default(), 0, Default::default(), 1024),
            system_reservation_context: SystemReservationContext::default(),
            allocations: Default::default(),
            pages_data: Default::default(),
            generated_dispatches: Default::default(),
            awakening: Default::default(),
            program_candidates_data: Default::default(),
            context_store: Default::default(),
        })
    }

    fn gas_amount(&self) -> GasAmount {
        GasCounter::new(0).to_amount()
    }

    fn pre_process_memory_accesses(
        _reads: &[MemoryInterval],
        _writes: &[MemoryInterval],
        _gas_left: &mut GasLeft,
    ) -> Result<(), ProcessAccessError> {
        Ok(())
    }
}
