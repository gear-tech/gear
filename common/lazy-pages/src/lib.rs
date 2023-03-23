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

//! Lazy pages support for runtime.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::ToString;
use core::fmt;
use gear_backend_common::{
    lazy_pages::{GlobalsAccessConfig, LazyPagesWeights, Status},
    memory::ProcessAccessError,
};
use gear_common::Origin;
use gear_core::{
    gas::GasLeft,
    ids::ProgramId,
    memory::{GearPage, HostPointer, Memory, MemoryInterval, PageU32Size, WasmPage},
};
use gear_runtime_interface::{gear_ri, LazyPagesProgramContext, LazyPagesRuntimeContext};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use sp_std::{vec, vec::Vec};

fn mprotect_lazy_pages(mem: &mut impl Memory, protect: bool) {
    if mem.get_buffer_host_addr().is_none() {
        return;
    }

    // Cannot panic, unless OS has some problems with pages protection.
    gear_ri::mprotect_lazy_pages(protect);
}

/// Try to enable and initialize lazy pages env
pub fn try_to_enable_lazy_pages(prefix: [u8; 32]) -> bool {
    let ctx = LazyPagesRuntimeContext {
        page_sizes: vec![WasmPage::size(), GearPage::size()],
        global_names: vec![
            GLOBAL_NAME_GAS.to_string(),
            GLOBAL_NAME_ALLOWANCE.to_string(),
        ],
        pages_storage_prefix: prefix.to_vec(),
    };

    gear_ri::init_lazy_pages(ctx)
}

/// Protect and save storage keys for pages which has no data
pub fn init_for_program(
    mem: &mut impl Memory,
    program_id: ProgramId,
    stack_end: Option<WasmPage>,
    globals_config: GlobalsAccessConfig,
    weights: LazyPagesWeights,
) {
    let weights = [
        weights.signal_read,
        weights.signal_write,
        weights.signal_write_after_read,
        weights.host_func_read,
        weights.host_func_write,
        weights.host_func_write_after_read,
        weights.load_page_storage_data,
    ]
    .map(|w| w.one())
    .to_vec();

    let ctx = LazyPagesProgramContext {
        wasm_mem_addr: mem.get_buffer_host_addr(),
        wasm_mem_size: mem.size().raw(),
        stack_end: stack_end.map(|p| p.raw()),
        program_id: <[u8; 32]>::from(program_id.into_origin()).into(),
        globals_config,
        weights,
    };

    // Cannot panic unless OS allocates buffer in not aligned by native page addr, or
    // something goes wrong with pages protection.
    gear_ri::init_lazy_pages_for_program(ctx);
}

/// Remove lazy-pages protection, returns wasm memory begin addr
pub fn remove_lazy_pages_prot(mem: &mut impl Memory) {
    mprotect_lazy_pages(mem, false);
}

/// Protect lazy-pages and set new wasm mem addr and size,
/// if they have been changed.
pub fn update_lazy_pages_and_protect_again(
    mem: &mut impl Memory,
    old_mem_addr: Option<HostPointer>,
    old_mem_size: WasmPage,
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
            new_mem_addr
        );

        Some(new_mem_addr)
    } else {
        None
    };

    let new_mem_size = mem.size();
    let changed_size = (new_mem_size > old_mem_size).then_some(new_mem_size.raw());

    if !matches!((changed_addr, changed_size), (None, None)) {
        gear_ri::change_wasm_memory_addr_and_size(changed_addr, changed_size)
    }

    mprotect_lazy_pages(mem, true);
}

/// Returns list of released pages numbers.
pub fn get_write_accessed_pages() -> Vec<GearPage> {
    gear_ri::write_accessed_pages()
        .into_iter()
        .map(|p| {
            GearPage::new(p)
                .unwrap_or_else(|_| unreachable!("Lazy pages backend returns wrong pages"))
        })
        .collect()
}

/// Returns lazy pages actual status.
pub fn get_status() -> Status {
    gear_ri::lazy_pages_status().0
}

/// Pre-process memory access in syscalls in lazy-pages.
pub fn pre_process_memory_accesses(
    reads: &[MemoryInterval],
    writes: &[MemoryInterval],
    gas_left: &mut GasLeft,
) -> Result<(), ProcessAccessError> {
    let (gas_left_new, res) = gear_ri::pre_process_memory_accesses(reads, writes, (*gas_left,));
    *gas_left = gas_left_new;
    res
}
