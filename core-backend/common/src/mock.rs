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
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, GetGasAmount, IntoExtInfo,
    SystemReservationContext, TerminationReason,
};
use alloc::collections::BTreeSet;
use codec::{Decode, Encode};
use core::fmt;
use gear_core::{
    costs::RuntimeCosts,
    env::Ext,
    gas::{GasAmount, GasCounter},
    ids::{MessageId, ProgramId, ReservationId},
    memory::{Memory, WasmPageNumber},
    message::{HandlePacket, InitPacket, ReplyPacket, StatusCode},
    reservation::GasReserver,
};
use gear_core_errors::{CoreError, ExtError, MemoryError};

/// Mock error
#[derive(Debug, Encode, Decode)]
pub struct Error;

impl fmt::Display for Error {
    fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
        todo!()
    }
}

impl CoreError for Error {
    fn forbidden_function() -> Self {
        todo!()
    }
}

impl AsTerminationReason for Error {
    fn as_termination_reason(&self) -> Option<&TerminationReason> {
        todo!()
    }
}

impl IntoExtError for Error {
    fn into_ext_error(self) -> Result<ExtError, Self> {
        todo!()
    }
}

/// Mock ext
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct MockExt(BTreeSet<&'static str>);

impl Ext for MockExt {
    type Error = Error;

    fn alloc(
        &mut self,
        _pages: WasmPageNumber,
        _mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
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
    fn exit(&mut self) -> Result<(), Self::Error> {
        Ok(())
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
    fn free(&mut self, _page: WasmPageNumber) -> Result<(), Self::Error> {
        Ok(())
    }
    fn debug(&mut self, _data: &str) -> Result<(), Self::Error> {
        Ok(())
    }
    fn read(&mut self) -> Result<&[u8], Self::Error> {
        Ok(&[])
    }
    fn size(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn charge_gas(&mut self, _amount: u64) -> Result<(), Self::Error> {
        Ok(())
    }
    fn charge_gas_runtime(&mut self, _costs: RuntimeCosts) -> Result<(), Self::Error> {
        Ok(())
    }
    fn refund_gas(&mut self, _amount: u64) -> Result<(), Self::Error> {
        Ok(())
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
    fn random(&self) -> (&[u8], u32) {
        ([0u8; 32].as_ref(), 0)
    }
    fn leave(&mut self) -> Result<(), Self::Error> {
        Ok(())
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
    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.0
    }
    fn reserve_gas(&mut self, _amount: u64, _duration: u32) -> Result<ReservationId, Self::Error> {
        Ok(ReservationId::default())
    }
    fn unreserve_gas(&mut self, _id: ReservationId) -> Result<u64, Self::Error> {
        Ok(0)
    }

    fn counters(&self) -> (u64, u64) {
        (0, 0)
    }

    fn update_counters(&mut self, _gas: u64, _allowance: u64) {}

    fn out_of_allowance(&mut self) -> Self::Error {
        Error
    }

    fn out_of_gas(&mut self) -> Self::Error {
        Error
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
}

impl IntoExtInfo<<MockExt as Ext>::Error> for MockExt {
    fn into_ext_info(self, _memory: &impl Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        Ok(ExtInfo {
            gas_amount: GasAmount::from(GasCounter::new(0)),
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

    fn into_gas_amount(self) -> gear_core::gas::GasAmount {
        GasAmount::from(GasCounter::new(0))
    }

    fn last_error(&self) -> Result<&gear_core_errors::ExtError, Error> {
        Ok(&ExtError::SyscallUsage)
    }

    fn trap_explanation(&self) -> Option<crate::TrapExplanation> {
        None
    }
}

impl GetGasAmount for MockExt {
    fn gas_amount(&self) -> GasAmount {
        GasAmount::from(GasCounter::new(0))
    }
}
