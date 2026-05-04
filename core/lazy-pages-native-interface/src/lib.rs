// This file is part of Gear.

// Copyright (C) 2024-2025 Gear Technologies Inc.
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
    costs::LazyPagesCosts,
    ids::ActorId,
    memory::{HostPointer, Memory, MemoryInterval},
    pages::{GearPage, WasmPage, WasmPagesAmount},
    program::MemoryInfix,
};
use gear_lazy_pages_common::{GlobalsAccessConfig, LazyPagesInterface, ProcessAccessError, Status};

pub struct LazyPagesNative;

impl LazyPagesInterface for LazyPagesNative {
    fn try_to_enable_lazy_pages(_prefix: [u8; 32]) -> bool {
        let err_msg = "LazyPagesNative::try_to_enable_lazy_pages: this function should not be called in native";

        log::error!("{err_msg}");
        unreachable!("{err_msg}")
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
        let wasm_mem_addr = mem.get_buffer_host_addr(ctx).map(|addr| {
            usize::try_from(addr).unwrap_or_else(|err| {
                let err_msg = format!(
                    "LazyPagesNative::init_for_program: can't convert native address to usize. \
                    Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
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
        .unwrap_or_else(|err| {
            let err_msg = format!(
                "LazyPagesNative::init_for_program: can't initialize lazy pages for program. \
                Program id - {program_id:?}, memory infix - {memory_infix:?}. Got error - {err:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });
    }

    fn remove_lazy_pages_prot<Context>(_ctx: &mut Context, _mem: &mut impl Memory<Context>) {
        gear_lazy_pages::unset_lazy_pages_protection().unwrap_or_else(|err| {
            let err_msg = format!(
                "LazyPagesNative::remove_lazy_pages_prot: can't unset lazy pages protection. \
                    Got error - {err:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        });
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
                let err_msg = format!(
                    "LazyPagesNative::update_lazy_pages_and_protect_again: can't convert native address to usize. \
                        Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            })
        });
        let size: u32 = mem.size(ctx).into();
        gear_lazy_pages::change_wasm_mem_addr_and_size(addr, Some(size))
            .unwrap_or_else(|err| {
                let err_msg = format!(
                    "LazyPagesNative::update_lazy_pages_and_protect_again: can't change wasm memory address and size. \
                        Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            });
        gear_lazy_pages::set_lazy_pages_protection()
            .unwrap_or_else(|err| {
                let err_msg = format!(
                    "LazyPagesNative::update_lazy_pages_and_protect_again: can't set lazy pages protection. \
                        Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
        });
    }

    fn get_write_accessed_pages() -> Vec<GearPage> {
        gear_lazy_pages::write_accessed_pages()
            .unwrap_or_else(|err| {
                let err_msg = format!(
                    "LazyPagesNative::get_write_accessed_pages: can't get write accessed pages. \
                        Got error - {err:?}"
                );

                log::error!("{err_msg}");
                unreachable!("{err_msg}")
            })
            .into_iter()
            .map(|p| {
                GearPage::try_from(p)
                    .unwrap_or_else(|err| {
                        let err_msg = format!(
                            "LazyPagesNative::get_write_accessed_pages: incorrect accessed page number. \
                                Got error - {err:?}"
                        );

                        log::error!("{err_msg}");
                        unreachable!("{err_msg}")
                    })
            })
            .collect()
    }

    fn get_status() -> Status {
        gear_lazy_pages::status().unwrap_or_else(|err| {
            let err_msg = format!(
                "LazyPagesNative::get_status: can't get lazy pages status. \
                        Got error - {err:?}"
            );

            log::error!("{err_msg}");
            unreachable!("{err_msg}")
        })
    }

    fn pre_process_memory_accesses(
        reads: &[MemoryInterval],
        writes: &[MemoryInterval],
        gas_counter: &mut u64,
    ) -> Result<(), ProcessAccessError> {
        gear_lazy_pages::pre_process_memory_accesses(reads, writes, gas_counter)
    }
}
