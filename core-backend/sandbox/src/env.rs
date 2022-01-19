// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! sp-sandbox environment for running a module.

use crate::{funcs, memory::MemoryWrap};
use alloc::{boxed::Box, collections::BTreeMap, format, vec::Vec};
use core::marker::PhantomData;
use gear_backend_common::{
    funcs as common_funcs, BackendError, BackendReport, Environment, ExtInfo, TerminationReason,
};
use gear_core::{
    env::{Ext, LaterExt},
    memory::{Memory, PageBuf, PageNumber},
};
use sp_sandbox::{
    default_executor::{EnvironmentDefinitionBuilder, Instance, Memory as DefaultExecutorMemory},
    SandboxEnvironmentBuilder, SandboxInstance, SandboxMemory,
};

use common::Origin;
use core::convert::TryFrom;

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E: Ext>(PhantomData<E>);

impl<E: Ext> Default for SandboxEnvironment<E> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

pub struct Runtime<E: Ext + Into<ExtInfo>> {
    pub(crate) ext: LaterExt<E>,
    pub(crate) lazy_pages_enabled: bool,
    pub(crate) trap: Option<&'static str>,
}

impl<E: Ext + Into<ExtInfo> + 'static> Runtime<E> {
    fn new(ext: E, lazy_pages_enabled: bool) -> Self {
        let mut later_ext = LaterExt::default();
        later_ext.set(ext);

        Self {
            ext: later_ext,
            lazy_pages_enabled,
            trap: None,
        }
    }
}

impl<E: Ext + Into<ExtInfo> + 'static> SandboxEnvironment<E> {
    fn setup(
        &self,
        memory: &dyn Memory,
    ) -> Result<EnvironmentDefinitionBuilder<Runtime<E>>, &'static str> {
        let mem = memory
            .as_any()
            .downcast_ref::<DefaultExecutorMemory>()
            .ok_or("Memory is not SandboxMemory")?;

        let mut env_builder = EnvironmentDefinitionBuilder::new();

        env_builder.add_memory("env", "memory", mem.clone());
        env_builder.add_host_func("env", "alloc", funcs::alloc);
        env_builder.add_host_func("env", "free", funcs::free);
        env_builder.add_host_func("env", "gr_block_height", funcs::block_height);
        env_builder.add_host_func("env", "gr_block_timestamp", funcs::block_timestamp);
        env_builder.add_host_func("env", "gr_exit_code", funcs::exit_code);
        env_builder.add_host_func("env", "gr_send", funcs::send);
        env_builder.add_host_func("env", "gr_send_commit", funcs::send_commit);
        env_builder.add_host_func("env", "gr_send_init", funcs::send_init);
        env_builder.add_host_func("env", "gr_send_push", funcs::send_push);
        env_builder.add_host_func("env", "gr_read", funcs::read);
        env_builder.add_host_func("env", "gr_size", funcs::size);
        env_builder.add_host_func("env", "gr_source", funcs::source);
        env_builder.add_host_func("env", "gr_program_id", funcs::program_id);
        env_builder.add_host_func("env", "gr_value", funcs::value);
        env_builder.add_host_func("env", "gr_reply", funcs::reply);
        env_builder.add_host_func("env", "gr_reply_commit", funcs::reply_commit);
        env_builder.add_host_func("env", "gr_reply_to", funcs::reply_to);
        env_builder.add_host_func("env", "gr_reply_push", funcs::reply_push);
        env_builder.add_host_func("env", "gr_debug", funcs::debug);
        env_builder.add_host_func("env", "gr_gas_available", funcs::gas_available);
        env_builder.add_host_func("env", "gr_msg_id", funcs::msg_id);
        env_builder.add_host_func("env", "gr_leave", funcs::leave);
        env_builder.add_host_func("env", "gr_wait", funcs::wait);
        env_builder.add_host_func("env", "gr_wake", funcs::wake);
        env_builder.add_host_func("env", "gas", funcs::gas);

        Ok(env_builder)
    }
}

impl<E: Ext + Into<ExtInfo> + 'static> Environment<E> for SandboxEnvironment<E> {
    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    fn setup_and_execute(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &mut BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> Result<BackendReport, BackendError> {
        let env_builder = match self.setup(memory) {
            Ok(builder) => builder,
            Err(e) => {
                let info: ExtInfo = ext.into();

                return Err(BackendError {
                    reason: e,
                    description: None,
                    gas_amount: info.gas_amount,
                });
            }
        };

        // In case any page buf is none we suppose that it's candidate to be lazy page
        let lazy_pages_enabled = if !memory_pages.iter().any(|(_, buf)| buf.is_none()) {
            log::debug!("lazy-pages: there is no pages to be lazy");
            false
        } else if cfg!(feature = "disable_lazy_pages")
            || cfg!(target_family = "wasm")
            || !gear_ri::gear_ri::init_lazy_pages()
        {
            // In case we don't support lazy-pages then we loads them all now.

            // TODO: to support in Wasm runtime we must change embedded executor to host executor.
            // TODO: also we cannot support for validators in relay-chain,
            // but it can be fixed in future only.

            log::debug!("lazy-pages: not unsupported here");
            let prog_id_hash = ext.program_id().into_origin();
            memory_pages
                .iter_mut()
                .filter(|(_x, y)| y.is_none())
                .for_each(|(x, y)| {
                    let data = common::get_program_page_data(prog_id_hash, x.raw())
                        .expect("Page data must be in storage");
                    *y = Option::from(Box::from(PageBuf::try_from(data).expect(
                        "Must be able to convert vec to PageBuf, may be vec has wrong size",
                    )));
                });
            false
        } else {
            log::debug!("lazy-pages: enabled");
            true
        };

        let mut runtime = Runtime::new(ext, lazy_pages_enabled);

        let mut instance = Instance::new(binary, &env_builder, &mut runtime).map_err(|e| {
            let info: ExtInfo = runtime.ext.unset().into();
            BackendError {
                reason: "Unable to instanciate module",
                description: Some(format!("{:?}", e).into()),
                gas_amount: info.gas_amount,
            }
        })?;

        // Set module memory.
        memory.set_pages(memory_pages).map_err(|e| {
            let info: ExtInfo = runtime.ext.unset().into();

            BackendError {
                reason: "Unable to set module memory",
                description: Some(format!("{:?}", e).into()),
                gas_amount: info.gas_amount,
            }
        })?;

        // Actions for lazy pages: protect and save storage keys.
        if lazy_pages_enabled {
            let lazy_pages = memory_pages
                .iter()
                .filter(|(_num, buf)| buf.is_none())
                .map(|(num, _buf)| num.raw())
                .collect::<Vec<u32>>();
            let prog_id_hash = runtime
                .ext
                .with(|ext| ext.program_id())
                .expect("Must be correct")
                .into_origin();
            let wasm_mem_begin_addr = runtime
                .ext
                .with(|ext| ext.get_wasm_memory_begin_addr())
                .expect("Must be correct");

            gear_ri::gear_ri::set_wasm_mem_begin_addr(wasm_mem_begin_addr as u64);

            lazy_pages.iter().for_each(|p| {
                common::save_page_lazy_info(prog_id_hash, *p);
            });

            gear_ri::gear_ri::mprotect_wasm_pages(
                wasm_mem_begin_addr as u64,
                &lazy_pages,
                false,
                false,
                false,
            );
        }

        let res = instance.invoke(entry_point, &[], &mut runtime);

        if lazy_pages_enabled {
            // Loads data for released lazy pages. Data which was before execution.
            let released_pages = gear_ri::gear_ri::get_released_pages();
            released_pages.into_iter().for_each(|page| {
                let data = gear_ri::gear_ri::get_released_page_old_data(page);
                memory_pages.insert(
                    (page).into(),
                    Option::from(Box::new(
                        PageBuf::try_from(data).expect("Must be able to convert"),
                    )),
                );
            });

            // Removes protections from lazy pages
            let wasm_mem_begin_addr = runtime
                .ext
                .with(|ext| ext.get_wasm_memory_begin_addr())
                .expect("Must be correct") as u64;
            let lazy_pages = gear_ri::gear_ri::get_wasm_lazy_pages_numbers();
            gear_ri::gear_ri::mprotect_wasm_pages(
                wasm_mem_begin_addr,
                &lazy_pages,
                true,
                true,
                false,
            );

            gear_ri::gear_ri::reset_lazy_pages_info();
        }

        let info: ExtInfo = runtime.ext.unset().into();

        let termination = if res.is_err() {
            let mut reason = None;

            if let Some(trap) = runtime.trap {
                if common_funcs::is_wait_trap(trap) {
                    reason = Some(TerminationReason::Manual { wait: true });
                } else if common_funcs::is_leave_trap(trap) {
                    reason = Some(TerminationReason::Manual { wait: false });
                }
            };

            reason.unwrap_or_else(|| TerminationReason::Trap {
                explanation: info.trap_explanation,
                description: runtime.trap.map(Into::into),
            })
        } else {
            TerminationReason::Success
        };

        Ok(BackendReport { termination, info })
    }

    fn create_memory(&self, total_pages: u32) -> Result<Box<dyn Memory>, &'static str> {
        Ok(Box::new(MemoryWrap::new(
            SandboxMemory::new(total_pages, None).map_err(|_| "Create env memory fail")?,
        )))
    }
}
