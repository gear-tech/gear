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

use core::fmt;
use gear_backend_common::{
    lazy_pages::{GlobalsConfig, LazyPagesWeights, Status},
    memory::OutOfMemoryAccessError,
};
use gear_common::Origin;
use gear_core::{
    ids::ProgramId,
    memory::{HostPointer, Memory, MemoryInterval, PageNumber, WasmPageNumber},
};
use gear_runtime_interface::{gear_ri, LazyPagesProgramContext};
use sp_std::vec::Vec;

fn mprotect_lazy_pages(mem: &mut impl Memory, protect: bool) {
    if mem.get_buffer_host_addr().is_none() {
        return;
    }

    // Cannot panic, unless OS has some problems with pages protection.
    gear_ri::mprotect_lazy_pages(protect);
}

/// Try to enable and initialize lazy pages env
pub fn try_to_enable_lazy_pages(pages_final_prefix: [u8; 32]) -> bool {
    gear_ri::init_lazy_pages(pages_final_prefix)
}

/// Protect and save storage keys for pages which has no data
pub fn init_for_program(
    mem: &mut impl Memory,
    program_id: ProgramId,
    stack_end: Option<WasmPageNumber>,
    globals_config: GlobalsConfig,
    lazy_pages_weights: LazyPagesWeights,
) {
    let wasm_mem_addr = mem.get_buffer_host_addr();
    let wasm_mem_size = mem.size();
    let program_id = <[u8; 32]>::from(program_id.into_origin()).into();

    let ctx = LazyPagesProgramContext {
        wasm_mem_addr,
        wasm_mem_size,
        stack_end,
        program_id,
        globals_config,
        lazy_pages_weights,
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
    old_mem_size: WasmPageNumber,
    new_mem_addr: HostPointer,
) {
    struct PointerDisplay(HostPointer);

    impl fmt::Debug for PointerDisplay {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{:#x}", self.0)
        }
    }

    if old_mem_addr
        .map(|addr| new_mem_addr != addr)
        .unwrap_or(true)
    {
        log::debug!(
            "backend executor has changed wasm mem buff: from {:?} to {:?}",
            old_mem_addr.map(PointerDisplay),
            new_mem_addr
        );

        // Cannot panic, unless OS allocates wasm mem buffer
        // in not aligned by native page addr.
        gear_ri::set_wasm_mem_begin_addr(new_mem_addr);
    }

    let new_mem_size = mem.size();
    if new_mem_size > old_mem_size {
        gear_ri::set_wasm_mem_size(new_mem_size.into());
    }

    mprotect_lazy_pages(mem, true);
}

/// Returns list of released pages numbers.
pub fn get_released_pages() -> Vec<PageNumber> {
    gear_ri::get_released_pages()
}

/// Returns lazy pages actual status.
pub fn get_status() -> Option<Status> {
    gear_ri::get_lazy_pages_status()
}

/// Pre-process memory access in syscalls in lazy-pages.
pub fn pre_process_memory_accesses(
    reads: &[MemoryInterval],
    writes: &[MemoryInterval],
) -> Result<(), OutOfMemoryAccessError> {
    // TODO: make wrapper to pass `&[MemoryInterval]` in runtime-interface (issue #2099).
    let reads = reads.iter().copied().map(Into::into).collect::<Vec<_>>();
    let writes = writes.iter().copied().map(Into::into).collect::<Vec<_>>();
    gear_ri::pre_process_memory_accesses(&reads, &writes)
}
