// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

use anyhow::{bail, Context};

use gear_wasm_gen::SyscallName;
use gear_wasm_instrument::{parity_wasm::elements::Module, GLOBAL_NAME_GAS};
use wasmi::{
    memory_units::Pages, ExternVal, FuncInstance, FuncRef, ImportsBuilder, MemoryInstance,
    MemoryRef, Module as WasmiModule, ModuleImportResolver, ModuleInstance, ModuleRef,
    RuntimeValue, Trap, TrapCode, ValueType,
};

use crate::{
    globals::{get_globals, globals_list, InstanceAccessGlobal},
    lazy_pages::{self, FuzzerLazyPagesContext},
    RunResult, Runner, INITIAL_PAGES, MODULE_ENV, PROGRAM_GAS,
};

use error::CustomHostError;
mod error;

struct Resolver {
    memory: MemoryRef,
}

impl ModuleImportResolver for Resolver {
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &wasmi::Signature,
    ) -> Result<FuncRef, wasmi::Error> {
        if field_name == SyscallName::SystemBreak.to_str() {
            Ok(FuncInstance::alloc_host(
                wasmi::Signature::new([ValueType::I32].as_slice(), None),
                0,
            ))
        } else {
            Err(wasmi::Error::Instantiation(format!(
                "Export '{field_name}' not found"
            )))
        }
    }

    fn resolve_memory(
        &self,
        _field_name: &str,
        _memory_type: &wasmi::MemoryDescriptor,
    ) -> Result<MemoryRef, wasmi::Error> {
        Ok(self.memory.clone())
    }
}

struct Externals {
    gr_system_break_idx: usize,
}

impl wasmi::Externals for Externals {
    fn invoke_index(
        &mut self,
        index: usize,
        _args: wasmi::RuntimeArgs,
    ) -> Result<Option<wasmi::RuntimeValue>, wasmi::Trap> {
        Err(if index == self.gr_system_break_idx {
            wasmi::Trap::host(CustomHostError::from("out of gas"))
        } else {
            TrapCode::Unreachable.into()
        })
    }
}

impl InstanceAccessGlobal for ModuleRef {
    fn set_global(&self, name: &str, value: i64) -> anyhow::Result<()> {
        let Some(ExternVal::Global(global)) = self.export_by_name(name) else {
            bail!("global '{name}' not found");
        };

        Ok(global.set(RuntimeValue::I64(value))?)
    }

    fn get_global(&self, name: &str) -> anyhow::Result<i64> {
        let Some(ExternVal::Global(global)) = self.export_by_name(name) else {
            bail!("global '{name}' not found");
        };

        let RuntimeValue::I64(v) = global.get() else {
            bail!("global is not an i64");
        };

        Ok(v)
    }
}

pub struct WasmiRunner;

impl Runner for WasmiRunner {
    fn run(module: &Module) -> anyhow::Result<RunResult> {
        let wasmi_module =
            WasmiModule::from_buffer(module.clone().into_bytes().map_err(anyhow::Error::msg)?)
                .context("failed to load wasm")?;

        let memory = MemoryInstance::alloc(Pages(INITIAL_PAGES as usize), None)
            .context("failed to allocate memory")?;

        let mem_ptr = memory.direct_access().as_ref().as_ptr() as usize;
        let mem_size = memory.direct_access().as_ref().len();

        let resolver = Resolver { memory };
        let imports = ImportsBuilder::new().with_resolver(MODULE_ENV, &resolver);

        let instance = ModuleInstance::new(&wasmi_module, &imports)
            .context("failed to instantiate wasm module")?
            .assert_no_start();

        instance
            .set_global(GLOBAL_NAME_GAS, PROGRAM_GAS)
            .context("failed to set gas")?;

        lazy_pages::init_fuzzer_lazy_pages(FuzzerLazyPagesContext {
            instance: Box::new(instance.clone()),
            memory_range: mem_ptr..(mem_ptr + mem_size),
            pages: Default::default(),
            globals_list: globals_list(module),
        });

        if let Err(error) = instance.invoke_export(
            "init",
            &[],
            &mut Externals {
                gr_system_break_idx: 0,
            },
        ) {
            if let wasmi::Error::Trap(Trap::Host(_)) = error {
                log::info!("out of gas");
            } else {
                Err(error)?;
            }
        }

        let result = RunResult {
            gas_global: instance.get_global(GLOBAL_NAME_GAS)?,
            pages: lazy_pages::get_touched_pages(),
            globals: get_globals(&instance, module).context("failed to get globals")?,
        };

        Ok(result)
    }
}
