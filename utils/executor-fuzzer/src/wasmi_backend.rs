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

use error::CustomHostError;
use gear_wasm_instrument::{parity_wasm::elements::Module, GLOBAL_NAME_GAS};

use gear_wasm_gen::SyscallName;
use wasmi::{
    memory_units::Pages, ExternVal, FuncInstance, FuncRef, ImportsBuilder, MemoryInstance,
    MemoryRef, Module as WasmiModule, ModuleImportResolver, ModuleInstance, ModuleRef,
    RuntimeValue, TrapCode, ValueType,
};

use crate::{
    globals::InstanceAccessGlobal, lazy_pages, RunResult, Runner, INITIAL_PAGES, PROGRAM_GAS,
};

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
            wasmi::Trap::host(CustomHostError::from("out off gas"))
        } else {
            TrapCode::Unreachable.into()
        })
    }
}

impl InstanceAccessGlobal for ModuleRef {
    fn set_global(&self, name: &str, value: i64) -> anyhow::Result<()> {
        let Some(ExternVal::Global(global)) = self.export_by_name(name) else {
            panic!("global '{name}' not found");
        };

        global
            .set(RuntimeValue::I64(value))
            .expect("failed to set global");
        Ok(())
    }

    fn get_global(&self, name: &str) -> anyhow::Result<i64> {
        let Some(ExternVal::Global(global)) = self.export_by_name(name) else {
            panic!("global '{name}' not found");
        };

        if let RuntimeValue::I64(v) = global.get() {
            Ok(v)
        } else {
            panic!("Global is not an i64");
        }
    }
}

pub struct WasmiRunner;

impl Runner for WasmiRunner {
    fn run(module: &Module) -> anyhow::Result<RunResult> {
        let wasmi_module =
            WasmiModule::from_buffer(module.clone().into_bytes().expect("valid bytes"))
                .expect("failed to load wasm");

        let memory = MemoryInstance::alloc(Pages(INITIAL_PAGES as usize), None)
            .expect("failed to allocate memory");

        let mem_ptr = memory.direct_access().as_ref().as_ptr() as usize;
        let mem_size = memory.direct_access().as_ref().len();

        let resolver = Resolver { memory };
        let imports = ImportsBuilder::new().with_resolver("env", &resolver);

        let instance = ModuleInstance::new(&wasmi_module, &imports)
            .expect("failed to instantiate wasm module")
            .assert_no_start();

        let Some(ExternVal::Global(gear_gas)) = instance.export_by_name(GLOBAL_NAME_GAS) else {
            panic!("failed to get gas global");
        };
        gear_gas
            .set(RuntimeValue::I64(PROGRAM_GAS))
            .expect("failed to set gas");

        lazy_pages::init_fuzzer_lazy_pages(lazy_pages::FuzzerLazyPagesContext {
            instance: Box::new(instance.clone()),
            memory_range: mem_ptr..(mem_ptr + mem_size),
            pages: Default::default(),
        });

        if let Err(error) = instance.invoke_export(
            "init",
            &[],
            &mut Externals {
                gr_system_break_idx: 0,
            },
        ) {
            if let wasmi::Error::Trap(wasmi::Trap::Host(msg)) = error {
                log::error!("{msg}");
            } else {
                panic!("failed to run wasm: {:?}", error);
            }
        }

        let result = RunResult {
            gas_global: instance.get_global(GLOBAL_NAME_GAS)?,
            pages: lazy_pages::get_touched_pages(),
        };

        Ok(result)
    }
}
