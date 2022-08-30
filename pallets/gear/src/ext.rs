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

use alloc::collections::BTreeSet;
use core_processor::{Ext, ProcessorContext, ProcessorError, ProcessorExt};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, ExtInfo, IntoExtInfo, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    costs::RuntimeCosts,
    env::Ext as EnvExt,
    gas::GasAmount,
    ids::{MessageId, ProgramId},
    memory::{Memory, PageBuf, WasmPageNumber},
    message::{ExitCode, HandlePacket, ReplyPacket},
};
use gear_core_errors::{CoreError, ExtError, MemoryError};
use gear_lazy_pages_common as lazy_pages;
use sp_std::collections::btree_map::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::From)]
pub enum Error {
    #[from]
    #[display(fmt = "{}", _0)]
    Processor(ProcessorError),
    #[from]
    #[display(fmt = "{}", _0)]
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

/// Ext with lazy pages support.
pub struct LazyPagesExt {
    inner: Ext,
}

impl IntoExtInfo for LazyPagesExt {
    fn into_ext_info(self, memory: &impl Memory) -> Result<ExtInfo, (MemoryError, GasAmount)> {
        let ProcessorContext {
            allocations_context,
            message_context,
            gas_counter,
            program_candidates_data,
            ..
        } = self.inner.context;

        // Accessed pages are all pages except current lazy pages
        let static_pages = allocations_context.static_pages();
        let (initial_allocations, allocations) = allocations_context.into_parts();
        let mut accessed_pages = lazy_pages::get_released_pages();
        accessed_pages.retain(|p| {
            let wasm_page = p.to_wasm_page();
            wasm_page < static_pages || allocations.contains(&wasm_page)
        });

        log::trace!("accessed pages numbers = {:?}", accessed_pages);

        let mut accessed_pages_data = BTreeMap::new();
        for page in accessed_pages {
            let mut buf = PageBuf::new_zeroed();
            if let Err(err) = memory.read(page.offset(), buf.as_mut_slice()) {
                return Err((err, gas_counter.into()));
            }
            accessed_pages_data.insert(page, buf);
        }

        let (outcome, context_store) = message_context.drain();
        let (generated_dispatches, awakening) = outcome.drain();

        let info = ExtInfo {
            gas_amount: gas_counter.into(),
            allocations: allocations.ne(&initial_allocations).then_some(allocations),
            pages_data: accessed_pages_data,
            generated_dispatches,
            awakening,
            context_store,
            program_candidates_data,
        };
        Ok(info)
    }

    fn into_gas_amount(self) -> gear_core::gas::GasAmount {
        self.inner.context.gas_counter.into()
    }

    fn last_error(&self) -> Option<&ExtError> {
        self.inner.last_error()
    }

    fn trap_explanation(&self) -> Option<TrapExplanation> {
        self.inner.trap_explanation()
    }
}

impl ProcessorExt for LazyPagesExt {
    type Error = Error;
    const LAZY_PAGES_ENABLED: bool = true;

    fn new(context: ProcessorContext) -> Self {
        Self {
            inner: Ext::new(context),
        }
    }

    fn lazy_pages_init_for_program(
        mem: &impl Memory,
        prog_id: ProgramId,
        stack_end: Option<WasmPageNumber>,
    ) -> Result<(), Self::Error> {
        lazy_pages::init_for_program(mem, prog_id, stack_end).map_err(Into::into)
    }

    fn lazy_pages_post_execution_actions(mem: &impl Memory) -> Result<(), Self::Error> {
        lazy_pages::remove_lazy_pages_prot(mem).map_err(Into::into)
    }
}

impl EnvExt for LazyPagesExt {
    type Error = Error;

    fn alloc(
        &mut self,
        pages_num: WasmPageNumber,
        mem: &mut impl Memory,
    ) -> Result<WasmPageNumber, Self::Error> {
        self.charge_gas_runtime(RuntimeCosts::Alloc)?;

        // Greedily charge gas for allocations
        self.charge_gas((pages_num.0 as u64).saturating_mul(self.inner.context.config.alloc_cost))?;
        // Greedily charge gas for grow
        self.charge_gas(
            (pages_num.0 as u64).saturating_mul(self.inner.context.config.mem_grow_cost),
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
            .context
            .allocations_context
            .alloc(pages_num, mem)
            .map_err(ExtError::Memory);

        let page_number = self.inner.return_and_store_err(result)?;

        // Add new allocations to lazy pages.
        // Protect all lazy pages including new allocations.
        lazy_pages::update_lazy_pages_and_protect_again(mem, old_mem_addr, old_mem_size)?;

        // Returns back greedily used gas for grow
        let new_mem_size = mem.size();
        let grow_pages_num = new_mem_size - old_mem_size;
        let mut gas_to_return_back = self
            .inner
            .context
            .config
            .mem_grow_cost
            .saturating_mul((pages_num - grow_pages_num).0 as u64);

        // Returns back greedily used gas for allocations
        let first_page = page_number;
        let last_page = first_page + pages_num - 1.into();
        let mut new_allocated_pages_num = WasmPageNumber(0);
        for page in first_page.0..=last_page.0 {
            if !self
                .inner
                .context
                .allocations_context
                .is_init_page(page.into())
            {
                new_allocated_pages_num = new_allocated_pages_num + 1.into();
            }
        }
        gas_to_return_back = gas_to_return_back.saturating_add(
            self.inner
                .context
                .config
                .alloc_cost
                .saturating_mul((pages_num - new_allocated_pages_num).0 as u64),
        );

        self.refund_gas(gas_to_return_back)?;

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

    fn reply_to(&mut self) -> Result<Option<MessageId>, Self::Error> {
        self.inner.reply_to().map_err(Error::Processor)
    }

    fn source(&mut self) -> Result<ProgramId, Self::Error> {
        self.inner.source().map_err(Error::Processor)
    }

    fn exit(&mut self) -> Result<(), Self::Error> {
        self.inner.exit().map_err(Error::Processor)
    }

    fn exit_code(&mut self) -> Result<Option<ExitCode>, Self::Error> {
        self.inner.exit_code().map_err(Error::Processor)
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

    fn charge_gas(&mut self, val: u64) -> Result<(), Self::Error> {
        self.inner.charge_gas(val).map_err(Error::Processor)
    }

    fn refund_gas(&mut self, val: u64) -> Result<(), Self::Error> {
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
    ) -> Result<ProgramId, Self::Error> {
        self.inner.create_program(packet).map_err(Error::Processor)
    }

    fn charge_gas_runtime(&mut self, costs: RuntimeCosts) -> Result<(), Self::Error> {
        self.inner
            .charge_gas_runtime(costs)
            .map_err(Error::Processor)
    }

    fn forbidden_funcs(&self) -> &BTreeSet<&'static str> {
        &self.inner.context.forbidden_funcs
    }
}
