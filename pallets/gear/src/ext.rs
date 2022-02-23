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

use common::{lazy_pages, ExitCode};
use core_processor::{
    configs::{AllocationsConfig, BlockInfo},
    BlakeMessageIdGenerator, Ext, ProcessorExt,
};
use gear_backend_common::ExtInfo;
use gear_core::{
    env::Ext as EnvExt,
    gas::{GasAmount, GasCounter, ValueCounter},
    memory::{MemoryContext, PageBuf, PageNumber},
    message::{MessageContext, MessageId, MessageState, OutgoingPacket, ReplyPacket},
    program::ProgramId,
};
use sp_std::{boxed::Box, collections::btree_map::BTreeMap, vec, vec::Vec};

/// Ext with lazy pages support
pub struct LazyPagesExt {
    inner: Ext,
    lazy_pages_enabled: bool,
}

impl From<LazyPagesExt> for ExtInfo {
    fn from(ext: LazyPagesExt) -> ExtInfo {
        let mut accessed_pages_numbers = ext.inner.memory_context.allocations().clone();

        // accessed pages are all pages except current lazy pages
        if ext.lazy_pages_enabled {
            let lazy_pages_numbers = lazy_pages::get_lazy_pages_numbers();
            lazy_pages_numbers.into_iter().for_each(|p| {
                accessed_pages_numbers.remove(&p.into());
            });
        }

        let mut accessed_pages = BTreeMap::new();
        for page in accessed_pages_numbers {
            let mut buf = vec![0u8; PageNumber::size()];
            ext.get_mem(page.offset(), &mut buf);
            accessed_pages.insert(page, buf);
        }

        let nonce = ext.inner.message_context.nonce();

        let (
            MessageState {
                outgoing,
                reply,
                awakening,
            },
            store,
        ) = ext.inner.message_context.drain();

        let gas_amount: GasAmount = ext.inner.gas_counter.into();

        let trap_explanation = ext.inner.error_explanation;

        ExtInfo {
            gas_amount,
            pages: ext.inner.memory_context.allocations().clone(),
            accessed_pages,
            outgoing,
            reply,
            awakening,
            nonce,
            payload_store: Some(store),
            trap_explanation,
            exit_argument: ext.inner.exit_argument,
        }
    }
}

impl ProcessorExt for LazyPagesExt {
    fn new(
        gas_counter: GasCounter,
        value_counter: ValueCounter,
        memory_context: MemoryContext,
        message_context: MessageContext<BlakeMessageIdGenerator>,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        error_explanation: Option<&'static str>,
        exit_argument: Option<ProgramId>,
    ) -> Self {
        Self {
            inner: Ext {
                gas_counter,
                value_counter,
                memory_context,
                message_context,
                block_info,
                config,
                existential_deposit,
                error_explanation,
                exit_argument,
            },
            lazy_pages_enabled: false,
        }
    }

    fn try_to_enable_lazy_pages(
        &mut self,
        program_id: ProgramId,
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
    ) -> bool {
        self.lazy_pages_enabled = lazy_pages::try_to_enable_lazy_pages(program_id, memory_pages);
        self.lazy_pages_enabled
    }

    fn protect_pages_and_init_info(
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        prog_id: ProgramId,
        wasm_mem_begin_addr: usize,
    ) {
        lazy_pages::protect_pages_and_init_info(memory_pages, prog_id, wasm_mem_begin_addr);
    }

    fn post_execution_actions(
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        wasm_mem_begin_addr: usize,
    ) {
        lazy_pages::post_execution_actions(memory_pages, wasm_mem_begin_addr);
    }

    fn remove_lazy_pages_prot(mem_addr: usize) {
        lazy_pages::remove_lazy_pages_prot(mem_addr);
    }

    fn protect_lazy_pages_and_update_wasm_mem_addr(old_mem_addr: usize, new_mem_addr: usize) {
        lazy_pages::protect_lazy_pages_and_update_wasm_mem_addr(old_mem_addr, new_mem_addr);
    }

    fn get_lazy_pages_numbers() -> Vec<u32> {
        lazy_pages::get_lazy_pages_numbers()
    }
}

impl EnvExt for LazyPagesExt {
    fn alloc(&mut self, pages_num: PageNumber) -> Result<PageNumber, &'static str> {
        // Greedily charge gas for allocations
        self.charge_gas(pages_num.raw() * self.inner.config.alloc_cost as u32)?;
        // Greedily charge gas for grow
        self.charge_gas(pages_num.raw() * self.inner.config.mem_grow_cost as u32)?;

        let old_mem_size = self.inner.memory_context.memory().size().raw();

        // New pages allocation may change wasm memory buffer location.
        // So, if lazy-pages are enabled we remove protections from lazy-pages
        // and returns it back for new wasm memory buffer pages.
        // Also we correct lazy-pages info if need.
        let old_mem_addr = if self.lazy_pages_enabled {
            let mem_addr = self.get_wasm_memory_begin_addr();
            LazyPagesExt::remove_lazy_pages_prot(mem_addr);
            mem_addr
        } else {
            0
        };

        let result = self
            .inner
            .memory_context
            .alloc(pages_num)
            .map_err(|_e| "Allocation error");

        if result.is_err() {
            return self.inner.return_and_store_err(result);
        }

        if self.lazy_pages_enabled {
            let new_mem_addr = self.get_wasm_memory_begin_addr();
            LazyPagesExt::protect_lazy_pages_and_update_wasm_mem_addr(old_mem_addr, new_mem_addr);
        }

        // Returns back greedily used gas for grow
        let new_mem_size = self.inner.memory_context.memory().size().raw();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back =
            self.inner.config.mem_grow_cost * (pages_num.raw() - grow_pages_num) as u64;

        // Returns back greedily used gas for allocations
        let first_page = result.unwrap().raw();
        let last_page = first_page + pages_num.raw() - 1;
        let mut new_alloced_pages_num = 0;
        for page in first_page..=last_page {
            if !self.inner.memory_context.is_init_page(page.into()) {
                new_alloced_pages_num += 1;
            }
        }
        gas_to_return_back +=
            self.inner.config.alloc_cost * (pages_num.raw() - new_alloced_pages_num) as u64;

        self.refund_gas(gas_to_return_back as u32)?;

        self.inner.return_and_store_err(result)
    }

    fn block_height(&self) -> u32 {
        self.inner.block_height()
    }

    fn block_timestamp(&self) -> u64 {
        self.inner.block_timestamp()
    }

    fn send_init(&mut self) -> Result<usize, &'static str> {
        self.inner.send_init()
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), &'static str> {
        self.inner.send_push(handle, buffer)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), &'static str> {
        self.inner.reply_push(buffer)
    }

    fn send_commit(
        &mut self,
        handle: usize,
        msg: OutgoingPacket,
    ) -> Result<MessageId, &'static str> {
        self.inner.send_commit(handle, msg)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, &'static str> {
        self.inner.reply_commit(msg)
    }

    fn reply_to(&self) -> Option<(MessageId, ExitCode)> {
        self.inner.reply_to()
    }

    fn source(&mut self) -> ProgramId {
        self.inner.source()
    }

    fn exit(&mut self, value_destination: ProgramId) -> Result<(), &'static str> {
        self.inner.exit(value_destination)
    }

    fn message_id(&mut self) -> MessageId {
        self.inner.message_id()
    }

    fn program_id(&self) -> ProgramId {
        self.inner.program_id()
    }

    fn free(&mut self, ptr: PageNumber) -> Result<(), &'static str> {
        self.inner.free(ptr)
    }

    fn debug(&mut self, data: &str) -> Result<(), &'static str> {
        self.inner.debug(data)
    }

    fn set_mem(&mut self, ptr: usize, val: &[u8]) {
        self.inner.set_mem(ptr, val)
    }

    fn get_mem(&self, ptr: usize, buffer: &mut [u8]) {
        self.inner.get_mem(ptr, buffer);
    }

    fn msg(&mut self) -> &[u8] {
        self.inner.msg()
    }

    fn charge_gas(&mut self, val: u32) -> Result<(), &'static str> {
        self.inner.charge_gas(val)
    }

    fn refund_gas(&mut self, val: u32) -> Result<(), &'static str> {
        self.inner.refund_gas(val)
    }

    fn gas_available(&self) -> u64 {
        self.inner.gas_available()
    }

    fn value(&self) -> u128 {
        self.inner.value()
    }

    fn leave(&mut self) -> Result<(), &'static str> {
        self.inner.leave()
    }

    fn wait(&mut self) -> Result<(), &'static str> {
        self.inner.wait()
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), &'static str> {
        self.inner.wake(waker_id)
    }

    fn get_wasm_memory_begin_addr(&self) -> usize {
        self.inner.get_wasm_memory_begin_addr()
    }

    fn value_available(&self) -> u128 {
        self.inner.value_available()
    }
}
