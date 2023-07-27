// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! Runtime interface for gear node

#![allow(useless_deprecated, deprecated)]
#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use gear_backend_common::{
    lazy_pages::{GlobalsAccessConfig, Status},
    memory::ProcessAccessError,
    LimitedStr,
};
use gear_core::{
    gas::GasLeft,
    memory::{HostPointer, MemoryInterval},
};
use sp_runtime_interface::{
    pass_by::{Codec, PassBy},
    runtime_interface,
};

extern crate alloc;
use alloc::string::String;

#[cfg(feature = "std")]
use gear_lazy_pages as lazy_pages;

pub use sp_std::{convert::TryFrom, result::Result, vec::Vec};

mod gear_sandbox;
#[cfg(feature = "std")]
pub use gear_sandbox::init as sandbox_init;
pub use gear_sandbox::sandbox;

static_assertions::const_assert!(
    core::mem::size_of::<HostPointer>() >= core::mem::size_of::<usize>()
);

#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = codec)]
pub struct LazyPagesProgramContext {
    /// Wasm program memory addr.
    pub wasm_mem_addr: Option<HostPointer>,
    /// Wasm program memory size.
    pub wasm_mem_size: u32,
    /// Wasm program stack end page.
    pub stack_end: Option<u32>,
    /// Wasm program id.
    pub program_id: Vec<u8>,
    /// Globals config to access globals inside lazy-pages.
    pub globals_config: GlobalsAccessConfig,
    /// Lazy-pages access weights.
    pub weights: Vec<u64>,
}

impl PassBy for LazyPagesProgramContext {
    type PassBy = Codec<LazyPagesProgramContext>;
}

#[derive(Debug, Clone, Encode, Decode)]
#[codec(crate = codec)]
pub struct LazyPagesRuntimeContext {
    pub page_sizes: Vec<u32>,
    pub global_names: Vec<LimitedStr<'static>>,
    pub pages_storage_prefix: Vec<u8>,
}

impl PassBy for LazyPagesRuntimeContext {
    type PassBy = Codec<LazyPagesRuntimeContext>;
}

pub fn deserialize_mem_intervals(bytes: &[u8], intervals: &mut Vec<MemoryInterval>) {
    for chunk in bytes.chunks(8) {
        intervals.push(MemoryInterval::from_bytes(chunk));
    }
}

/// Runtime interface for gear node and runtime.
/// Note: name is expanded as gear_ri
#[runtime_interface]
pub trait GearRI {
    fn pre_process_memory_accesses(
        reads: &[u8],
        writes: &[u8],
        gas_couunter: u64,
    ) -> (u64, Result<(), ProcessAccessError>) {
        let reads_len = reads.len();
        let writes_len = writes.len();
        assert!(reads_len % 8 == 0);
        assert!(writes_len % 8 == 0);
        let mut reads_intervals = Vec::with_capacity(reads_len / 8);
        deserialize_mem_intervals(&reads, &mut reads_intervals);
        let mut writes_intervals = Vec::with_capacity(writes_len / 8);
        deserialize_mem_intervals(&writes, &mut writes_intervals);

        let mut gas_left = GasLeft {
            gas: gas_couunter,
            allowance: gas_couunter,
        };

        let res = lazy_pages::pre_process_memory_accesses(
            &reads_intervals,
            &writes_intervals,
            &mut gas_left,
        );

        let GasLeft { gas, allowance } = gas_left;
        (gas.min(allowance), res)
    }

    fn lazy_pages_status() -> (Status,) {
        (lazy_pages::status()
            .unwrap_or_else(|err| unreachable!("Cannot get lazy-pages status: {err}")),)
    }

    fn init_lazy_pages(ctx: LazyPagesRuntimeContext) -> bool {
        use lazy_pages::LazyPagesVersion;

        lazy_pages::init(
            LazyPagesVersion::Version1,
            ctx.page_sizes,
            ctx.global_names,
            ctx.pages_storage_prefix,
        )
        .map_err(|err| log::error!("Cannot initialize lazy-pages: {}", err))
        .is_ok()
    }

    /// Init lazy-pages.
    /// Returns whether initialization was successful.
    #[version(2)]
    fn init_lazy_pages(ctx: LazyPagesRuntimeContext) -> bool {
        use lazy_pages::LazyPagesVersion;

        lazy_pages::init(
            LazyPagesVersion::Version2,
            ctx.page_sizes,
            ctx.global_names,
            ctx.pages_storage_prefix,
        )
        .map_err(|err| log::error!("Cannot initialize lazy-pages: {}", err))
        .is_ok()
    }

    /// Init lazy pages context for current program.
    /// Panic if some goes wrong during initialization.
    fn init_lazy_pages_for_program(ctx: LazyPagesProgramContext) {
        let wasm_mem_addr = ctx.wasm_mem_addr.map(|addr| {
            usize::try_from(addr)
                .unwrap_or_else(|err| unreachable!("Cannot cast wasm mem addr to `usize`: {}", err))
        });

        lazy_pages::initialize_for_program(
            wasm_mem_addr,
            ctx.wasm_mem_size,
            ctx.stack_end,
            ctx.program_id,
            Some(ctx.globals_config),
            ctx.weights,
        )
        .map_err(|e| e.to_string())
        .expect("Cannot initialize lazy pages for current program");
    }

    /// Mprotect all wasm mem buffer except released pages.
    /// If `protect` argument is true then restrict all accesses to pages,
    /// else allows read and write accesses.
    fn mprotect_lazy_pages(protect: bool) {
        if protect {
            lazy_pages::set_lazy_pages_protection()
        } else {
            lazy_pages::unset_lazy_pages_protection()
        }
        .map_err(|err| err.to_string())
        .expect("Cannot set/unset mprotection for lazy pages");
    }

    fn change_wasm_memory_addr_and_size(addr: Option<HostPointer>, size: Option<u32>) {
        // `as usize` is safe, because of const assert above.
        gear_lazy_pages::change_wasm_mem_addr_and_size(addr.map(|addr| addr as usize), size)
            .unwrap_or_else(|err| unreachable!("Cannot set new wasm addr and size: {err}"));
    }

    fn write_accessed_pages() -> Vec<u32> {
        lazy_pages::write_accessed_pages()
            .unwrap_or_else(|err| unreachable!("Cannot get write accessed pages: {err}"))
    }

    // Bellow goes deprecated runtime interface functions.
}
