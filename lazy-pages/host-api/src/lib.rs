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

use gear_core::{
    limited::LimitedStr,
    memory::{HostPointer, MemoryInterval},
};
use gear_lazy_pages::{LazyPagesStorage, LazyPagesVersion};
use gear_lazy_pages_common::{GlobalsAccessConfig, Status};
use parity_scale_codec::{Decode, Encode};
#[cfg(feature = "pass-by")]
use sp_runtime_interface::pass_by::{Codec, PassBy};

#[derive(Debug, Clone, Encode, Decode)]
pub struct LazyPagesProgramContext {
    /// Wasm program memory addr.
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm program memory size.
    pub wasm_mem_size: u32,
    /// Wasm program stack end page.
    pub stack_end: Option<u32>,
    /// The field contains prefix to a program's memory pages, i.e.
    /// `program_id` + `memory_infix`.
    pub program_key: Vec<u8>,
    /// Globals config to access globals inside lazy-pages.
    pub globals_config: GlobalsAccessConfig,
    /// Lazy-pages access costs.
    pub costs: Vec<u64>,
}

#[cfg(feature = "pass-by")]
impl PassBy for LazyPagesProgramContext {
    type PassBy = Codec<LazyPagesProgramContext>;
}

#[derive(Debug, Clone, Encode, Decode)]
pub struct LazyPagesInitContext {
    pub page_sizes: Vec<u32>,
    pub global_names: Vec<LimitedStr<'static>>,
    pub pages_storage_prefix: Vec<u8>,
}

impl From<gear_lazy_pages_common::LazyPagesInitContext> for LazyPagesInitContext {
    fn from(ctx: gear_lazy_pages_common::LazyPagesInitContext) -> Self {
        let gear_lazy_pages_common::LazyPagesInitContext {
            page_sizes,
            global_names,
            pages_storage_prefix,
        } = ctx;

        Self {
            page_sizes,
            global_names,
            pages_storage_prefix,
        }
    }
}

impl From<LazyPagesInitContext> for gear_lazy_pages_common::LazyPagesInitContext {
    fn from(ctx: LazyPagesInitContext) -> Self {
        let LazyPagesInitContext {
            page_sizes,
            global_names,
            pages_storage_prefix,
        } = ctx;

        Self {
            page_sizes,
            global_names,
            pages_storage_prefix,
        }
    }
}

#[cfg(feature = "pass-by")]
impl PassBy for LazyPagesInitContext {
    type PassBy = Codec<LazyPagesInitContext>;
}

pub fn pre_process_memory_accesses(reads: &[u8], writes: &[u8], gas_counter: &mut u64) -> u8 {
    let mem_interval_size = size_of::<MemoryInterval>();
    let reads_len = reads.len();
    let writes_len = writes.len();

    let mut reads_intervals = Vec::with_capacity(reads_len / mem_interval_size);
    deserialize_mem_intervals(reads, &mut reads_intervals);
    let mut writes_intervals = Vec::with_capacity(writes_len / mem_interval_size);
    deserialize_mem_intervals(writes, &mut writes_intervals);

    gear_lazy_pages::pre_process_memory_accesses(&reads_intervals, &writes_intervals, gas_counter)
        .map(|_| 0)
        .unwrap_or_else(|err| err.into())
}

pub fn lazy_pages_status() -> (Status,) {
    (gear_lazy_pages::status()
        .unwrap_or_else(|err| unreachable!("Cannot get lazy-pages status: {err}")),)
}

/// Init lazy-pages.
/// Returns whether initialization was successful.
pub fn init_lazy_pages<S: LazyPagesStorage + 'static>(
    ctx: LazyPagesInitContext,
    storage: S,
) -> bool {
    gear_lazy_pages::init(LazyPagesVersion::Version1, ctx.into(), storage)
        .map_err(|err| tracing::error!("Cannot initialize lazy-pages: {err}"))
        .is_ok()
}

/// Init lazy pages context for current program.
/// Panic if some goes wrong during initialization.
pub fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
    let wasm_mem_addr = ctx.wasm_mem_addr.map(|addr| {
        usize::try_from(addr)
            .unwrap_or_else(|err| unreachable!("Cannot cast wasm mem addr to `usize`: {}", err))
    });

    gear_lazy_pages::initialize_for_program(
        wasm_mem_addr,
        ctx.wasm_mem_size,
        ctx.stack_end,
        ctx.program_key,
        Some(ctx.globals_config),
        ctx.costs,
    )
    .map_err(|e| e.to_string())
    .expect("Cannot initialize lazy pages for current program");
}

/// Mprotect all wasm mem buffer except released pages.
/// If `protect` argument is true then restrict all accesses to pages,
/// else allows read and write accesses.
pub fn mprotect_lazy_pages(protect: bool) {
    if protect {
        gear_lazy_pages::set_lazy_pages_protection()
    } else {
        gear_lazy_pages::unset_lazy_pages_protection()
    }
    .map_err(|err| err.to_string())
    .expect("Cannot set/unset mprotection for lazy pages");
}

pub fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
    // `as usize` is safe, because of const assert above.
    gear_lazy_pages::change_wasm_mem_addr_and_size(addr.map(|addr| addr as usize), size)
        .unwrap_or_else(|err| unreachable!("Cannot set new wasm addr and size: {err}"));
}

pub fn write_accessed_pages() -> Vec<u32> {
    gear_lazy_pages::write_accessed_pages()
        .unwrap_or_else(|err| unreachable!("Cannot get write accessed pages: {err}"))
}

fn deserialize_mem_intervals(bytes: &[u8], intervals: &mut Vec<MemoryInterval>) {
    let mem_interval_size = size_of::<MemoryInterval>();
    for chunk in bytes.chunks_exact(mem_interval_size) {
        // can't panic because of chunks_exact
        intervals.push(MemoryInterval::try_from_bytes(chunk).unwrap());
    }
}
