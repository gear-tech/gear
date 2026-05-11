// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

// TODO (breathx): remove cloning of slices from wasm memory (unsafe casts).

use crate::host::{StoreData, context, threads::EthexeHostLazyPages};
use gear_lazy_pages::LazyPagesVersion;
use gear_lazy_pages_host_api::LazyPagesInitContext;
use wasmtime::{Caller, Linker};

pub fn link(linker: &mut Linker<StoreData>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "env",
        "ext_gear_ri_change_wasm_memory_addr_and_size_version_1",
        change_wasm_memory_addr_and_size,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_init_lazy_pages_version_1",
        init_lazy_pages,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_init_lazy_pages_for_program_version_1",
        init_lazy_pages_for_program,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_lazy_pages_status_version_1",
        lazy_pages_status,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_mprotect_lazy_pages_version_1",
        mprotect_lazy_pages,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_pre_process_memory_accesses_version_2",
        pre_process_memory_accesses,
    )?;
    linker.func_wrap(
        "env",
        "ext_gear_ri_write_accessed_pages_version_1",
        write_accessed_pages,
    )?;

    Ok(())
}

fn change_wasm_memory_addr_and_size(caller: Caller<'_, StoreData>, addr: i64, size: i64) {
    log::trace!(target: "host_call", "change_wasm_memory_addr_and_size(addr={addr:?}, size={size:?})");

    let memory = context::memory(caller);
    let addr = memory.decode_by_val(addr);
    let size = memory.decode_by_val(size);

    gear_lazy_pages_host_api::change_wasm_memory_addr_and_size(addr, size);
}

fn init_lazy_pages(caller: Caller<'_, StoreData>, ctx: i64) -> i32 {
    log::trace!(target: "host_call", "init_lazy_pages(ctx={ctx:?})");

    let ctx: LazyPagesInitContext = context::memory(caller).decode_by_val(ctx);

    gear_lazy_pages::init(LazyPagesVersion::Version1, ctx.into(), EthexeHostLazyPages)
        .map_err(|err| log::error!("Cannot initialize lazy-pages: {err}"))
        .is_ok() as i32
}

fn init_lazy_pages_for_program(caller: Caller<'_, StoreData>, ctx: i64) {
    log::trace!(target: "host_call", "init_lazy_pages_for_program(ctx={ctx:?})");

    let ctx = context::memory(caller).decode_by_val(ctx);

    gear_lazy_pages_host_api::init_lazy_pages_for_program(ctx);
}

fn lazy_pages_status(caller: Caller<'_, StoreData>) -> i64 {
    log::trace!(target: "host_call", "lazy_pages_status()");

    let status = gear_lazy_pages_host_api::lazy_pages_status();

    let res = context::memory(caller).allocate_and_write_val(status);

    log::trace!(target: "host_call", "lazy_pages_status(..) -> {res:?}");

    res
}

fn mprotect_lazy_pages(_caller: Caller<'_, StoreData>, protect: i32) {
    log::trace!(target: "host_call", "mprotect_lazy_pages(protect={protect:?})");

    gear_lazy_pages_host_api::mprotect_lazy_pages(protect != 0);
}

fn pre_process_memory_accesses(
    mut caller: Caller<'_, StoreData>,
    reads: i64,
    writes: i64,
    gas_bytes: u32,
) -> i32 {
    log::trace!(target: "host_call", "pre_process_memory_accesses(reads={reads:?}, writes={writes:?}, gas_bytes={gas_bytes:?})");

    let mut memory = context::memory(&mut caller);
    let reads = memory.slice_by_val(reads);
    let writes = memory.slice_by_val(writes);

    // read gas_bytes into `mut` variable because `pre_process_memory_accesses` updates
    // it, then write updated slice to memory.
    let mut gas_counter: u64 = memory.decode_by_max_len(gas_bytes);

    let res = gear_lazy_pages_host_api::pre_process_memory_accesses(reads, writes, &mut gas_counter)
        as i32;

    memory
        .slice_mut(gas_bytes, 8)
        .unwrap()
        .copy_from_slice(&gas_counter.to_le_bytes());
    log::trace!(target: "host_call", "pre_process_memory_accesses(..) -> {res:?}");

    res
}

fn write_accessed_pages(caller: Caller<'_, StoreData>) -> i64 {
    log::trace!(target: "host_call", "write_accessed_pages()");

    let pages = gear_lazy_pages_host_api::write_accessed_pages();

    let res = context::memory(caller).allocate_and_write_val(pages);

    log::trace!(target: "host_call", "write_accessed_pages(..) -> {res:?}");

    res
}
