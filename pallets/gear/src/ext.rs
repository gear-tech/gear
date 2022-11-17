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

use alloc::{collections::BTreeSet, vec::Vec};
use core_processor::{Ext, ProcessorContext, ProcessorError, ProcessorExt};
use gear_backend_common::{ExtInfo, GetGasAmount, IntoExtInfo, TrapExplanation};
use gear_core::{
    costs::RuntimeCosts,
    env::Ext as EnvExt,
    gas::GasAmount,
    ids::{MessageId, ProgramId, ReservationId},
    memory::{GrowHandler, Memory, PageNumber, WasmPageNumber},
    message::{HandlePacket, InitPacket, ReplyPacket, StatusCode},
};
use gear_core_errors::{ExtError, MemoryError};
use gear_lazy_pages_common as lazy_pages;

/// Ext with lazy pages support.
pub struct LazyPagesExt {
    inner: Ext,
}

impl IntoExtInfo<<LazyPagesExt as EnvExt>::Error> for LazyPagesExt {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        let pages_for_data = |static_pages: WasmPageNumber,
                              allocations: &BTreeSet<WasmPageNumber>|
         -> Vec<PageNumber> {
            // Accessed pages are all pages, that had been released and are in allocations set or static.
            let mut accessed_pages = lazy_pages::get_released_pages();
            accessed_pages.retain(|p| {
                let wasm_page = p.to_wasm_page();
                wasm_page < static_pages || allocations.contains(&wasm_page)
            });
            log::trace!("accessed pages numbers = {:?}", accessed_pages);
            accessed_pages
        };
        self.inner.into_ext_info_inner(memory, pages_for_data)
    }

    fn into_gas_amount(self) -> gear_core::gas::GasAmount {
        self.inner.context.gas_counter.into()
    }

    fn last_error(&self) -> Result<&ExtError, <LazyPagesExt as EnvExt>::Error> {
        self.inner.last_error()
    }

    fn trap_explanation(&self) -> Option<TrapExplanation> {
        self.inner.trap_explanation()
    }
}

impl GetGasAmount for LazyPagesExt {
    fn gas_amount(&self) -> GasAmount {
        let gas_counter = self.inner.context.gas_counter.clone();

        gas_counter.into()
    }
}

impl ProcessorExt for LazyPagesExt {
    const LAZY_PAGES_ENABLED: bool = true;

    fn new(context: ProcessorContext) -> Self {
        Self {
            inner: Ext::new(context),
        }
    }

    fn lazy_pages_init_for_program(
        mem: &mut impl Memory,
        prog_id: ProgramId,
        stack_end: Option<WasmPageNumber>,
    ) {
        lazy_pages::init_for_program(mem, prog_id, stack_end);
    }

    fn lazy_pages_post_execution_actions(mem: &mut impl Memory) {
        lazy_pages::remove_lazy_pages_prot(mem);
    }
}

struct GrowHandlerLazy {
    old_mem_addr: Option<u64>,
    old_mem_size: WasmPageNumber,
}

impl GrowHandler for GrowHandlerLazy {
    fn before_grow_action(mem: &mut impl Memory) -> Self {
        // New pages allocation may change wasm memory buffer location.
        // So we remove protections from lazy-pages
        // and then in `after_grow_action` we set protection back for new wasm memory buffer.
        let old_mem_addr = mem.get_buffer_host_addr();
        lazy_pages::remove_lazy_pages_prot(mem);
        Self {
            old_mem_addr,
            old_mem_size: mem.size(),
        }
    }
    fn after_grow_action(self, mem: &mut impl Memory) -> Result<(), MemoryError> {
        // Add new allocations to lazy pages.
        // Protect all lazy pages including new allocations.
        let new_mem_addr = mem
            .get_buffer_host_addr()
            .ok_or(MemoryError::MemSizeIsZeroAfterGrow)?;
        lazy_pages::update_lazy_pages_and_protect_again(
            mem,
            self.old_mem_addr,
            self.old_mem_size,
            new_mem_addr,
        );
        Ok(())
    }
}

impl EnvExt for LazyPagesExt {
    type Error = ProcessorError;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        self.inner.alloc_inner::<GrowHandlerLazy>(pages_num, mem)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        self.inner.block_height()
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        self.inner.block_timestamp()
    }

    fn origin(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.origin()
    }

    fn send_init(&mut self) -> Result<u32, Self::Error> {
        self.inner.send_init()
    }

    fn send_push(&mut self, handle: u32, buffer: &[u8]) -> Result<(), Self::Error> {
        self.inner.send_push(handle, buffer)
    }

    fn resend_push(&mut self, handle: u32, offset: u32, len: u32) -> Result<(), Self::Error> {
        self.inner.resend_push(handle, offset, len)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        self.inner.reply_push(buffer)
    }

    fn send_commit(
        &mut self,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.inner.send_commit(handle, msg, delay)
    }

    fn reservation_send_commit(
        &mut self,
        id: ReservationId,
        handle: u32,
        msg: HandlePacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.inner.reservation_send_commit(id, handle, msg, delay)
    }

    fn reply_commit(&mut self, msg: ReplyPacket, delay: u32) -> Result<MessageId, Self::Error> {
        self.inner.reply_commit(msg, delay)
    }

    fn reservation_reply_commit(
        &mut self,
        id: ReservationId,
        msg: ReplyPacket,
        delay: u32,
    ) -> Result<MessageId, Self::Error> {
        self.inner.reservation_reply_commit(id, msg, delay)
    }

    fn reply_to(&mut self) -> Result<MessageId, Self::Error> {
        self.inner.reply_to()
    }

    fn rereply_push(&mut self, offset: u32, len: u32) -> Result<(), Self::Error> {
        self.inner.rereply_push(offset, len)
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.source()
    }

    fn exit(&mut self) -> Result<(), Self::Error> {
        self.inner.exit()
    }

    fn status_code(&mut self) -> Result<StatusCode, Self::Error> {
        self.inner.status_code()
    }

    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        self.inner.message_id()
    }

    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.program_id()
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        self.inner.free(page)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.inner.debug(data)
    }

    fn read(&mut self) -> Result<&[u8], Self::Error> {
        self.inner.read()
    }

    fn size(&mut self) -> Result<usize, Self::Error> {
        self.inner.size()
    }

    fn charge_gas(&mut self, val: u64) -> Result<(), Self::Error> {
        self.inner.charge_gas(val)
    }

    fn refund_gas(&mut self, val: u64) -> Result<(), Self::Error> {
        self.inner.refund_gas(val)
    }

    fn random(&self) -> (&[u8], u32) {
        self.inner.random()
    }

    fn reserve_gas(&mut self, amount: u64, blocks: u32) -> Result<ReservationId, Self::Error> {
        self.inner.reserve_gas(amount, blocks)
    }

    fn unreserve_gas(&mut self, id: ReservationId) -> Result<u64, Self::Error> {
        self.inner.unreserve_gas(id)
    }

    fn system_reserve_gas(&mut self, amount: u64) -> Result<(), Self::Error> {
        self.inner.system_reserve_gas(amount)
    }

    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        self.inner.gas_available()
    }

    fn value(&mut self) -> Result<u128, Self::Error> {
        self.inner.value()
    }

    fn leave(&mut self) -> Result<(), Self::Error> {
        self.inner.leave()
    }

    fn wait(&mut self) -> Result<(), Self::Error> {
        self.inner.wait()
    }

    fn wait_for(&mut self, duration: u32) -> Result<(), Self::Error> {
        self.inner.wait_for(duration)
    }

    fn wait_up_to(&mut self, duration: u32) -> Result<bool, Self::Error> {
        self.inner.wait_up_to(duration)
    }

    fn wake(&mut self, waker_id: MessageId, delay: u32) -> Result<(), Self::Error> {
        self.inner.wake(waker_id, delay)
    }

    fn value_available(&mut self) -> Result<u128, Self::Error> {
        self.inner.value_available()
    }

    fn create_program(
        &mut self,
        packet: InitPacket,
        delay: u32,
    ) -> Result<(MessageId, ProgramId), Self::Error> {
        self.inner.create_program(packet, delay)
    }

    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error> {
        self.inner.charge_gas_runtime(costs)
    }

    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.inner.context.forbidden_funcs
    }

    fn counters(&self) -> (u64, u64) {
        self.inner.counters()
    }

    fn update_counters(&mut self, gas: u64, allowance: u64) {
        self.inner.update_counters(gas, allowance)
    }

    fn out_of_gas(&mut self) -> Self::Error {
        self.inner.out_of_gas()
    }

    fn out_of_allowance(&mut self) -> Self::Error {
        self.inner.out_of_allowance()
    }
}
