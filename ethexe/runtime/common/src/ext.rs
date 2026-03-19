// This file is part of Gear.
//
// Copyright (C) 2026 Gear Technologies Inc.
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

//! Externalities implementation for ethexe runtime.

use crate::RuntimeInterface;
use alloc::collections::btree_set::BTreeSet;
use core_processor::{
    BackendExternalities, CountersOwner, Ext as CoreExt, ExtInfo, Externalities, FallibleExtError,
    ProcessorContext, ProcessorExternalities, configs::SyscallName,
};
use gear_core::{
    buffer::PayloadSlice,
    costs::{CostToken, LazyPagesCosts},
    env_vars::EnvVars,
    gas::{ChargeError, CounterType, GasAmount, GasLeft},
    memory::{Memory, MemoryError, MemoryInterval},
    message::{HandlePacket, InitPacket, MessageContext, ReplyPacket},
    pages::WasmPage,
    program::MemoryInfix,
};
use gear_core_errors::{ExtError, ReplyCode};
use gear_lazy_pages_common::{GlobalsAccessConfig, ProcessAccessError};
use gprimitives::{ActorId, MessageId, ReservationId};

pub struct Ext<RI: RuntimeInterface> {
    core: CoreExt<RI::LazyPages>,
}

impl<RI: RuntimeInterface> ProcessorExternalities for Ext<RI> {
    fn new(context: ProcessorContext) -> Self {
        Self {
            core: CoreExt::<RI::LazyPages>::new(context),
        }
    }

    fn lazy_pages_init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        prog_id: ActorId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        lazy_pages_costs: LazyPagesCosts,
    ) {
        CoreExt::<RI::LazyPages>::lazy_pages_init_for_program(
            ctx,
            mem,
            prog_id,
            memory_infix,
            stack_end,
            globals_config,
            lazy_pages_costs,
        )
    }

    fn lazy_pages_post_execution_actions<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
    ) {
        CoreExt::<RI::LazyPages>::lazy_pages_post_execution_actions(ctx, mem)
    }

    fn lazy_pages_status() -> gear_lazy_pages_common::Status {
        CoreExt::<RI::LazyPages>::lazy_pages_status()
    }

    delegate::delegate! {
        to self.core {
            fn into_ext_info<Context>(
                self,
                ctx: &mut Context,
                memory: &impl Memory<Context>,
            ) -> Result<ExtInfo, MemoryError>;

        }
    }
}

impl<RI: RuntimeInterface> Externalities for Ext<RI> {
    type UnrecoverableError = <CoreExt<RI::LazyPages> as Externalities>::UnrecoverableError;
    type FallibleError = <CoreExt<RI::LazyPages> as Externalities>::FallibleError;
    type AllocError = <CoreExt<RI::LazyPages> as Externalities>::AllocError;

    delegate::delegate! {
        to self.core {
            fn alloc<Context>(&mut self, ctx: &mut Context, mem: &mut impl Memory<Context>, pages_num: u32) -> Result<WasmPage, Self::AllocError>;
            fn free(&mut self, page: WasmPage) -> Result<(), Self::AllocError>;
            fn free_range(&mut self, start: WasmPage, end: WasmPage) -> Result<(), Self::AllocError>;
            fn env_vars(&self, version: u32) -> Result<EnvVars, Self::UnrecoverableError>;
            fn block_height(&self) -> Result<u32, Self::UnrecoverableError>;
            fn block_timestamp(&self) -> Result<u64, Self::UnrecoverableError>;
            fn send_init(&mut self) -> Result<u32, Self::FallibleError>;
            fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::FallibleError>;
            fn send_commit(&mut self, handle: u32, msg: HandlePacket, delay: u32) -> Result<MessageId, Self::FallibleError>;
            fn send_push_input(&mut self, handle: u32, offset: u32, len: u32) -> Result<(), Self::FallibleError>;
            fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::FallibleError>;
            fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::FallibleError>;
            fn reply_to(&self) -> Result<MessageId, Self::FallibleError>;
            fn reply_push_input(&mut self, offset: u32, len: u32) -> Result<(), Self::FallibleError>;
            fn source(&self) -> Result<ActorId, Self::UnrecoverableError>;
            fn reply_code(&self) -> Result<ReplyCode, Self::FallibleError>;
            fn message_id(&self) -> Result<MessageId, Self::UnrecoverableError>;
            fn program_id(&self) -> Result<ActorId, Self::UnrecoverableError>;
            fn debug(&self, data: &str) -> Result<(), Self::UnrecoverableError>;
            fn payload_slice(&mut self, at: u32, len: u32) -> Result<PayloadSlice, Self::FallibleError>;
            fn size(&self) -> Result<usize, Self::UnrecoverableError>;
            fn gas_available(&self) -> Result<u64, Self::UnrecoverableError>;
            fn value(&self) -> Result<u128, Self::UnrecoverableError>;
            fn value_available(&self) -> Result<u128, Self::UnrecoverableError>;
            fn wait_for(&mut self, duration: u32) -> Result<(), Self::UnrecoverableError>;
            fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::UnrecoverableError>;
            fn forbidden_funcs(&self) -> &BTreeSet<SyscallName>;
            fn msg_ctx(&self) -> &MessageContext;
        }
    }

    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::FallibleError> {
        if delay != 0 {
            Err(FallibleExtError::Core(ExtError::Unsupported))
        } else {
            self.core.wake(waker_id, delay)
        }
    }

    fn reservation_send_commit(
        &mut self,
        _: ReservationId,
        _: u32,
        _: HandlePacket,
        _: u32,
    ) -> Result<MessageId, Self::FallibleError> {
        unreachable!("reservation_send_commit syscall is forbidden in ethexe runtime")
    }

    fn reservation_reply_commit(
        &mut self,
        _: ReservationId,
        _: ReplyPacket,
    ) -> Result<MessageId, Self::FallibleError> {
        unreachable!("reservation_reply_commit syscall is forbidden in ethexe runtime")
    }

    fn signal_from(&self) -> Result<MessageId, Self::FallibleError> {
        unreachable!("signal_from syscall is forbidden in ethexe runtime")
    }

    fn signal_code(&self) -> Result<gear_core_errors::SignalCode, Self::FallibleError> {
        unreachable!("signal_code syscall is forbidden in ethexe runtime")
    }

    fn wait(&mut self) -> Result<(), Self::UnrecoverableError> {
        unreachable!("wait syscall is forbidden in ethexe runtime")
    }

    fn random(&self) -> Result<(&[u8], u32), Self::UnrecoverableError> {
        // TODO: #5238 implement random data generation in ethexe runtime
        unreachable!("random syscall is forbidden in ethexe runtime")
    }

    fn create_program(
        &mut self,
        _: InitPacket,
        _: u32,
    ) -> Result<(MessageId, ActorId), Self::FallibleError> {
        // TODO: #5239 implement program creation in ethexe runtime
        unreachable!("create_program syscall is forbidden in ethexe runtime")
    }

    fn reply_deposit(&mut self, _: MessageId, _: u64) -> Result<(), Self::FallibleError> {
        unreachable!("reply_deposit syscall is forbidden in ethexe runtime")
    }
    fn reserve_gas(&mut self, _: u64, _: u32) -> Result<ReservationId, Self::FallibleError> {
        unreachable!("reserve_gas syscall is forbidden in ethexe runtime")
    }

    fn unreserve_gas(&mut self, _: ReservationId) -> Result<u64, Self::FallibleError> {
        unreachable!("unreserve_gas syscall is forbidden in ethexe runtime")
    }

    fn system_reserve_gas(&mut self, _: u64) -> Result<(), Self::FallibleError> {
        unreachable!("system_reserve_gas syscall is forbidden in ethexe runtime")
    }
}

impl<RI: RuntimeInterface> CountersOwner for Ext<RI> {
    delegate::delegate! {
        to self.core {
            fn charge_gas_for_token(&mut self, token: CostToken) -> Result<(), ChargeError>;
            fn charge_gas_if_enough(&mut self, amount: u64) -> Result<(), ChargeError>;
            fn gas_left(&self) -> GasLeft;
            fn current_counter_type(&self) -> CounterType;
            fn decrease_current_counter_to(&mut self, amount: u64);
            fn define_current_counter(&mut self) -> u64;
        }
    }
}

impl<RI: RuntimeInterface> BackendExternalities for Ext<RI> {
    delegate::delegate! {
        to self.core {
            fn gas_amount(&self) -> GasAmount;
            fn pre_process_memory_accesses(&mut self, reads: &[MemoryInterval], writes: &[MemoryInterval], gas_counter: &mut u64) -> Result<(), ProcessAccessError>;
        }
    }
}
