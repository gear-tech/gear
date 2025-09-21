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

//! Lazy pages support for runtime.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use gear_lazy_pages_common::LazyPagesInterface;

use alloc::format;
use byteorder::{ByteOrder, LittleEndian};
use core::fmt;
use gear_core::{
    costs::LazyPagesCosts,
    ids::ActorId,
    memory::{HostPointer, Memory, MemoryInterval},
    pages::{GearPage, WasmPage, WasmPagesAmount},
    program::MemoryInfix,
};
use gear_lazy_pages_common::{
    GlobalsAccessConfig, LazyPagesInitContext, ProcessAccessError, Status,
};
use gear_runtime_interface::{LazyPagesProgramContext, gear_ri};
use sp_std::vec::Vec;

pub struct LazyPagesRuntimeInterface;

impl LazyPagesInterface for LazyPagesRuntimeInterface {
    fn try_to_enable_lazy_pages(prefix: [u8; 32]) -> bool {
        gear_ri::init_lazy_pages(LazyPagesInitContext::new(prefix).into())
    }

    fn init_for_program<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        program_id: ActorId,
        memory_infix: MemoryInfix,
        stack_end: Option<WasmPage>,
        globals_config: GlobalsAccessConfig,
        costs: LazyPagesCosts,
    ) {
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

        let ctx = LazyPagesProgramContext {
            wasm_mem_addr: mem.get_buffer_host_addr(ctx),
            wasm_mem_size: mem.size(ctx).into(),
            stack_end: stack_end.map(|p| p.into()),
            program_key: {
                let memory_infix = memory_infix.inner().to_le_bytes();

                [program_id.as_ref(), memory_infix.as_ref()].concat()
            },
            globals_config,
            costs,
        };

        // Cannot panic unless OS allocates buffer in not aligned by native page addr, or
        // something goes wrong with pages protection.
        gear_ri::init_lazy_pages_for_program(ctx);
    }

    fn remove_lazy_pages_prot<Context>(ctx: &mut Context, mem: &mut impl Memory<Context>) {
        mprotect_lazy_pages(ctx, mem, false);
    }

    fn update_lazy_pages_and_protect_again<Context>(
        ctx: &mut Context,
        mem: &mut impl Memory<Context>,
        old_mem_addr: Option<HostPointer>,
        old_mem_size: WasmPagesAmount,
        new_mem_addr: HostPointer,
    ) {
        struct PointerDisplay(HostPointer);

        impl fmt::Debug for PointerDisplay {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{:#x}", self.0)
            }
        }

        let changed_addr = if old_mem_addr
            .map(|addr| new_mem_addr != addr)
            .unwrap_or(true)
        {
            log::debug!(
                "backend executor has changed wasm mem buff: from {:?} to {:?}",
                old_mem_addr.map(PointerDisplay),
                PointerDisplay(new_mem_addr)
            );

            Some(new_mem_addr)
        } else {
            None
        };

        let new_mem_size = mem.size(ctx);
        let changed_size = (new_mem_size > old_mem_size).then_some(new_mem_size.into());

        if !matches!((changed_addr, changed_size), (None, None)) {
            gear_ri::change_wasm_memory_addr_and_size(changed_addr, changed_size)
        }

        mprotect_lazy_pages(ctx, mem, true);
    }

    fn get_write_accessed_pages() -> Vec<GearPage> {
        gear_ri::write_accessed_pages()
            .into_iter()
            .map(|p| {
                GearPage::try_from(p).unwrap_or_else(|err| {
                    let err_msg = format!(
                        "LazyPagesRuntimeInterface::get_write_accessed_pages: Lazy pages backend return wrong write accessed pages. \
                        Got error - {err}"
                    );

                    log::error!("{err_msg}");
                    unreachable!("{err_msg}")
                })
            })
            .collect()
    }

    fn get_status() -> Status {
        gear_ri::lazy_pages_status().0
    }

    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        let serialized_reads = serialize_mem_intervals(reads);
        let serialized_writes = serialize_mem_intervals(writes);

        let mut gas_bytes = [0u8; 8];
        LittleEndian::write_u64(&mut gas_bytes, *gas_counter);

        let res = gear_ri::pre_process_memory_accesses(
            &serialized_reads,
            &serialized_writes,
            &mut gas_bytes,
        );

        *gas_counter = LittleEndian::read_u64(&gas_bytes);

        // if result can be converted to `ProcessAccessError` then it's an error
        if let Ok(err) = ProcessAccessError::try_from(res) {
            return Err(err);
        }
        Ok(())
    }
}

fn serialize_mem_intervals(intervals: &[MemoryInterval]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(size_of_val(intervals));
    for interval in intervals {
        bytes.extend_from_slice(&interval.to_bytes());
    }
    bytes
}

fn mprotect_lazy_pages<Context>(ctx: &mut Context, mem: &mut impl Memory<Context>, protect: bool) {
    if mem.get_buffer_host_addr(ctx).is_none() {
        return;
    }

    // Cannot panic, unless OS has some problems with pages protection.
    gear_ri::mprotect_lazy_pages(protect);
}
