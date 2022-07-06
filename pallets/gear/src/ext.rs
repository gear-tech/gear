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

use ::common::Origin;
use alloc::collections::BTreeSet;
use common::{lazy_pages, save_page_lazy_info};
use core::fmt;
use core_processor::{
    configs::{AllocationsConfig, BlockInfo},
    Ext, ProcessorError, ProcessorExt,
};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, IntoExtInfo, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    costs::HostFnWeights,
    env::Ext as EnvExt,
    gas::{GasAllowanceCounter, GasAmount, GasCounter, ValueCounter},
    ids::{CodeId, MessageId, ProgramId},
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{HandlePacket, MessageContext, ReplyPacket},
};
use gear_core_errors::{CoreError, ExtError, MemoryError};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Processor(ProcessorError),
    LazyPages(lazy_pages::Error),
}

impl CoreError for Error {}

impl IntoExtError for Error {
    fn into_ext_error(self) -> Result<ExtError, Self> {
        match self {
            Error::Processor(err) => Ok(err.into_ext_error()?),
            err => Err(err),
        }
    }
}

impl AsTerminationReason for Error {
    fn as_termination_reason(&self) -> Option<&TerminationReason> {
        match self {
            Error::Processor(err) => err.as_termination_reason(),
            Error::LazyPages(_) => None,
        }
    }
}

impl From<ProcessorError> for Error {
    fn from(err: ProcessorError) -> Self {
        Self::Processor(err)
    }
}

impl From<lazy_pages::Error> for Error {
    fn from(err: lazy_pages::Error) -> Self {
        Self::LazyPages(err)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Processor(err) => fmt::Display::fmt(err, f),
            Error::LazyPages(err) => fmt::Display::fmt(err, f),
        }
    }
}

/// Ext with lazy pages support.
pub struct LazyPagesExt {
    inner: Ext,
    // Pages which has been allocated during current execution.
    fresh_allocations: BTreeSet<WasmPageNumber>,
}

impl IntoExtInfo for LazyPagesExt {
    fn into_ext_info(
        self,
        memory: &dyn Memory,
    ) -> Result<(ExtInfo, Option<TrapExplanation>), (MemoryError, GasAmount)> {
        // Accessed pages are all pages except current lazy pages
        let allocations = self.inner.allocations_context.allocations().clone();
        let mut accessed_pages = lazy_pages::get_released_pages();
        accessed_pages.retain(|p| allocations.contains(&p.to_wasm_page()));

        log::trace!("accessed pages numbers = {:?}", accessed_pages);

        let mut accessed_pages_data = BTreeMap::new();
        for page in accessed_pages {
            let mut buf = PageBuf::new_zeroed();
            if let Err(err) = memory.read(page.offset(), buf.as_mut_slice()) {
                return Err((err, self.into_gas_amount()));
            }
            accessed_pages_data.insert(page, buf);
        }

        let (outcome, context_store) = self.inner.message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        let info = ExtInfo {
            gas_amount: self.inner.gas_counter.into(),
            allocations,
            pages_data: accessed_pages_data,
            generated_dispatches,
            awakening,
            context_store,
            program_candidates_data: self.inner.program_candidates_data,
        };
        let trap_explanation = self
            .inner
            .error_explanation
            .and_then(ProcessorError::into_trap_explanation);
        Ok((info, trap_explanation))
    }

    fn into_gas_amount(self) -> gear_core::gas::GasAmount {
        self.inner.gas_counter.into()
    }

    fn last_error(&self) -> Option<&ExtError> {
        self.inner.last_error()
    }
}

impl ProcessorExt for LazyPagesExt {
    type Error = Error;

    fn new(
        gas_counter: GasCounter,
        gas_allowance_counter: GasAllowanceCounter,
        value_counter: ValueCounter,
        allocations_context: AllocationsContext,
        message_context: MessageContext,
        block_info: BlockInfo,
        config: AllocationsConfig,
        existential_deposit: u128,
        origin: ProgramId,
        program_id: ProgramId,
        program_candidates_data: BTreeMap<CodeId, Vec<(ProgramId, MessageId)>>,
        host_fn_weights: HostFnWeights,
        forbidden_funcs: BTreeSet<&'static str>,
        mailbox_threshold: u64,
    ) -> Self {
        assert!(cfg!(feature = "lazy-pages"));
        Self {
            inner: Ext {
                gas_counter,
                gas_allowance_counter,
                value_counter,
                allocations_context,
                message_context,
                block_info,
                config,
                existential_deposit,
                error_explanation: None,
                origin,
                program_id,
                program_candidates_data,
                host_fn_weights,
                forbidden_funcs,
                mailbox_threshold,
            },
            fresh_allocations: Default::default(),
        }
    }

    fn is_lazy_pages_enabled() -> bool {
        true
    }

    fn check_lazy_pages_consistent_state() -> bool {
        lazy_pages::is_lazy_pages_enabled()
    }

    fn lazy_pages_protect_and_init_info(
        mem: &dyn Memory,
        memory_pages: impl Iterator<Item = PageNumber>,
        prog_id: ProgramId,
    ) -> Result<(), Self::Error> {
        lazy_pages::protect_pages_and_init_info(mem, memory_pages, prog_id)
            .map_err(Error::LazyPages)
    }

    fn lazy_pages_post_execution_actions(
        mem: &dyn Memory,
        memory_pages: &mut BTreeMap<PageNumber, PageBuf>,
    ) -> Result<(), Self::Error> {
        lazy_pages::post_execution_actions(mem, memory_pages).map_err(Error::LazyPages)
    }
}

impl EnvExt for LazyPagesExt {
    type Error = Error;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut dyn Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        // Greedily charge gas for allocations
        self.charge_gas(
            pages_num
                .0
                .saturating_mul(self.inner.config.alloc_cost as u32),
        )?;
        // Greedily charge gas for grow
        self.charge_gas(
            pages_num
                .0
                .saturating_mul(self.inner.config.mem_grow_cost as u32),
        )?;

        let old_mem_size = mem.size();

        // New pages allocation may change wasm memory buffer location.
        // So we remove protections from lazy-pages
        // and set protection back for new wasm memory buffer pages.
        // Also we correct lazy-pages info if need.
        let old_mem_addr = mem.get_buffer_host_addr();
        lazy_pages::remove_lazy_pages_prot(mem)?;

        let result = self
            .inner
            .allocations_context
            .alloc(pages_num, mem)
            .map_err(ExtError::Memory);

        let page_number = self.inner.return_and_store_err(result)?;

        // Add new allocations to lazy pages.
        // All pages except ones which has been already allocated,
        // during current execution.
        // This is because only such pages contains Default (zeros in WebAsm) page data.
        // Pages which has been already allocated may contain garbage.
        let id = self.inner.program_id.into_origin();
        let new_allocated_pages = (page_number.0..(page_number + pages_num).0).map(WasmPageNumber);
        for wasm_page in new_allocated_pages {
            if self.inner.allocations_context.is_init_page(wasm_page)
                || self.fresh_allocations.contains(&wasm_page)
            {
                continue;
            }
            self.fresh_allocations.insert(wasm_page);
            save_page_lazy_info(id, wasm_page.to_gear_pages_iter());
        }

        // Protect all lazy pages including new allocations
        lazy_pages::protect_lazy_pages_and_update_wasm_mem_addr(mem, old_mem_addr)?;

        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self
            .inner
            .config
            .mem_grow_cost
            .saturating_mul((pages_num - grow_pages_num).0 as u64);

        // Returns back greedily used gas for allocations
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_allocated_pages_num = WasmPageNumber(0);
        for page in first_page.0..=last_page.0 {
            if !self.inner.allocations_context.is_init_page(page.into()) {
                new_allocated_pages_num = new_allocated_pages_num + 1.into();
            }
        }
        gas_to_return_back = gas_to_return_back.saturating_add(
            self.inner
                .config
                .alloc_cost
                .saturating_mul((pages_num - new_allocated_pages_num).0 as u64),
        );

        self.refund_gas(gas_to_return_back as u32)?;

        Ok(page_number)
    }

    fn block_height(&mut self) -> Result<u32, Self::Error> {
        self.inner.block_height().map_err(Error::Processor)
    }

    fn block_timestamp(&mut self) -> Result<u64, Self::Error> {
        self.inner.block_timestamp().map_err(Error::Processor)
    }

    fn origin(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.origin().map_err(Error::Processor)
    }

    fn send_init(&mut self) -> Result<usize, Self::Error> {
        self.inner.send_init().map_err(Error::Processor)
    }

    fn send_push(&mut self, handle: usize, buffer: &[u8]) -> Result<(), Self::Error> {
        self.inner
            .send_push(handle, buffer)
            .map_err(Error::Processor)
    }

    fn reply_push(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
        self.inner.reply_push(buffer).map_err(Error::Processor)
    }

    fn send_commit(&mut self, handle: usize, msg: HandlePacket) -> Result<MessageId, Self::Error> {
        self.inner
            .send_commit(handle, msg)
            .map_err(Error::Processor)
    }

    fn reply_commit(&mut self, msg: ReplyPacket) -> Result<MessageId, Self::Error> {
        self.inner.reply_commit(msg).map_err(Error::Processor)
    }

    fn reply_to(&mut self) -> Result<Option<(MessageId, i32)>, Self::Error> {
        self.inner.reply_to().map_err(Error::Processor)
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.source().map_err(Error::Processor)
    }

    fn exit(&mut self) -> Result<(), Self::Error> {
        self.inner.exit().map_err(Error::Processor)
    }

    fn message_id(&mut self) -> Result<MessageId, Self::Error> {
        self.inner.message_id().map_err(Error::Processor)
    }

    fn program_id(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.program_id().map_err(Error::Processor)
    }

    fn free(&mut self, page: WasmPageNumber) -> Result<(), Self::Error> {
        self.inner.free(page).map_err(Error::Processor)
    }

    fn debug(&mut self, data: &str) -> Result<(), Self::Error> {
        self.inner.debug(data).map_err(Error::Processor)
    }

    fn msg(&mut self) -> &[u8] {
        self.inner.msg()
    }

    fn charge_gas(&mut self, val: u32) -> Result<(), Self::Error> {
        self.inner.charge_gas(val).map_err(Error::Processor)
    }

    fn refund_gas(&mut self, val: u32) -> Result<(), Self::Error> {
        self.inner.refund_gas(val).map_err(Error::Processor)
    }

    fn gas(&mut self, val: u32) -> Result<(), Self::Error> {
        self.inner.gas(val).map_err(Error::Processor)
    }

    fn gas_available(&mut self) -> Result<u64, Self::Error> {
        self.inner.gas_available().map_err(Error::Processor)
    }

    fn value(&mut self) -> Result<u128, Self::Error> {
        self.inner.value().map_err(Error::Processor)
    }

    fn leave(&mut self) -> Result<(), Self::Error> {
        self.inner.leave().map_err(Error::Processor)
    }

    fn wait(&mut self) -> Result<(), Self::Error> {
        self.inner.wait().map_err(Error::Processor)
    }

    fn wake(&mut self, waker_id: MessageId) -> Result<(), Self::Error> {
        self.inner.wake(waker_id).map_err(Error::Processor)
    }

    fn value_available(&mut self) -> Result<u128, Self::Error> {
        self.inner.value_available().map_err(Error::Processor)
    }

    fn create_program(
        &mut self,
        packet: gear_core::message::InitPacket,
    ) -> Result<(ProgramId, MessageId), Self::Error> {
        self.inner.create_program(packet).map_err(Error::Processor)
    }

    fn charge_gas_runtime(
        &mut self,
        costs: gear_core::costs::RuntimeCosts,
    ) -> Result<(), Self::Error> {
        self.inner
            .charge_gas_runtime(costs)
            .map_err(Error::Processor)
    }

    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.inner.forbidden_funcs
    }
}
