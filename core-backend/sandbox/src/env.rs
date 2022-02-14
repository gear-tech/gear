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
use alloc::{boxed::Box, collections::BTreeMap, format};
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

/// Environment to run one module at a time providing Ext.
pub struct SandboxEnvironment<E: Ext + Into<ExtInfo>> {
    runtime: Option<Runtime<E>>,
    instance: Option<Instance<Runtime<E>>>,
}

impl<E: Ext + Into<ExtInfo>> Default for SandboxEnvironment<E> {
    fn default() -> Self {
        Self {
            runtime: None,
            instance: None,
        }
    }
}

pub struct Runtime<E: Ext + Into<ExtInfo>> {
    pub(crate) ext: LaterExt<E>,
    pub(crate) trap: Option<&'static str>,
}

impl<E: Ext + Into<ExtInfo> + 'static> Runtime<E> {
    fn new(ext: E) -> Self {
        let mut later_ext = LaterExt::default();
        later_ext.set(ext);

        Self {
            ext: later_ext,
            trap: None,
        }
    }
}

impl<E: Ext + Into<ExtInfo> + 'static> Environment<E> for SandboxEnvironment<E> {
    fn setup(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        memory: &dyn Memory,
    ) -> Result<(), BackendError<'static>> {
        let mem = match memory.as_any().downcast_ref::<DefaultExecutorMemory>() {
            Some(x) => x,
            None => {
                let info: ExtInfo = ext.into();
                return Err(BackendError {
                    reason: "Memory is not SandboxMemory",
                    description: None,
                    gas_amount: info.gas_amount,
                });
            }
        };

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

        let mut runtime = Runtime::new(ext);

        let instance = Instance::new(binary, &env_builder, &mut runtime).map_err(|e| {
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

        self.runtime.replace(runtime);
        self.instance.replace(instance);

        Ok(())
    }

    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError> {
        let instance = self.instance.as_mut().expect("Must have instance");
        let runtime = self.runtime.as_mut().expect("Must have runtime");

        let res = instance.invoke(entry_point, &[], runtime);

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
