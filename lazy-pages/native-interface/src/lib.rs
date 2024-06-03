// This file is part of Gear.

// Copyright (C) 2024 Gear Technologies Inc.
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

//! Lazy pages support for native.

use gear_core::{
    ids::ProgramId,
    memory::{HostPointer, Memory, MemoryInterval},
    pages::{GearPage, WasmPage, WasmPagesAmount},
    program::MemoryInfix,
};
use gear_lazy_pages_common::{
    GlobalsAccessConfig, LazyPagesCosts, LazyPagesInterface, ProcessAccessError, Status,
};

pub struct LazyPagesNative;

impl LazyPagesInterface for LazyPagesNative {
    fn try_to_enable_lazy_pages(_prefix: [u8; 32]) -> bool {
        unreachable!("This function should not be called in native")
    }

    fn init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        program_id: ProgramId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        costs: LazyPagesCosts,
    ) {
        let wasm_mem_addr = mem.get_buffer_host_addr(ctx).map(|addr| {
            usize::try_from(addr).unwrap_or_else(|err| {
                unreachable!("can't convert native address to usize: {err:?}")
            })
        });
        let wasm_mem_size = mem.size(ctx).into();
        let program_key = {
            let memory_infix = memory_infix.inner().to_le_bytes();
            [program_id.as_ref(), memory_infix.as_ref()].concat()
        };
        let stack_end = stack_end.map(|page| page.into());
        let costs = [
            costs.signal_read,
            costs.signal_write,
            costs.signal_write_after_read,
            costs.host_func_read,
            costs.host_func_write,
            costs.host_func_write_after_read,
            costs.load_page_storage_data,
        ]
        .map(|w| w.cost_for_one())
        .to_vec();

        gear_lazy_pages::initialize_for_program(
            wasm_mem_addr,
            wasm_mem_size,
            stack_end,
            program_key,
            Some(globals_config),
            costs,
        )
        .unwrap_or_else(|err| unreachable!("can't initialize lazy pages for program: {err}"));
    }

    fn remove_lazy_pages_prot<Context>(_ctx: &mut Context, _mem: &mut impl Memory<Context>) {
        gear_lazy_pages::unset_lazy_pages_protection()
            .unwrap_or_else(|err| unreachable!("can't unset lazy pages protection: {err}"));
    }

    fn update_lazy_pages_and_protect_again<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        _old_mem_addr: Option<HostPointer>,
        _old_mem_size: WasmPagesAmount,
        _new_mem_addr: HostPointer,
    ) {
        let addr = mem.get_buffer_host_addr(ctx).map(|addr| {
            usize::try_from(addr).unwrap_or_else(|err| {
                unreachable!("can't convert native address to usize: {err:?}")
            })
        });
        let size: u32 = mem.size(ctx).into();
        gear_lazy_pages::change_wasm_mem_addr_and_size(addr, Some(size))
            .unwrap_or_else(|err| unreachable!("can't change wasm memory address and size: {err}"));
        gear_lazy_pages::set_lazy_pages_protection()
            .unwrap_or_else(|err| unreachable!("can't set lazy pages protection: {err}"));
    }

    fn get_write_accessed_pages() -> Vec<GearPage> {
        gear_lazy_pages::write_accessed_pages()
            .unwrap_or_else(|err| unreachable!("can't get write accessed pages: {err}"))
            .into_iter()
            .map(|p| {
                GearPage::try_from(p)
                    .unwrap_or_else(|err| unreachable!("incorrect accessed page number: {err}"))
            })
            .collect()
    }

    fn get_status() -> Status {
        gear_lazy_pages::status()
            .unwrap_or_else(|err| unreachable!("can't get lazy pages status: {err}"))
    }

    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        gear_lazy_pages::pre_process_memory_accesses(reads, writes, gas_counter)
    }
}
