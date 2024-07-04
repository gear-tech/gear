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

use anyhow::{bail, Context, Result};

use gear_wasm_gen::SyscallName;
use gear_wasm_instrument::{parity_wasm::elements::Module, GLOBAL_NAME_GAS};
use sandbox_wasmer::{
    Exports, Extern, Function, FunctionType, ImportObject, Instance, Memory, MemoryType,
    Module as WasmerModule, RuntimeError, Singlepass, Store, Type, Universal, Val,
};

use crate::{
    globals::{get_globals, globals_list, InstanceAccessGlobal},
    lazy_pages::{self, FuzzerLazyPagesContext},
    RunResult, Runner, INITIAL_PAGES, MODULE_ENV, PROGRAM_GAS,
};

impl InstanceAccessGlobal for Instance {
    fn set_global(&self, name: &str, value: i64) -> Result<()> {
        let global = self.exports.get_global(name)?;
        global.set(Val::I64(value))?;
        Ok(())
    }

    fn get_global(&self, name: &str) -> Result<i64> {
        let global = self.exports.get_global(name)?;
        let Val::I64(v) = global.get() else {
            bail!("global is not an i64")
        };

        Ok(v)
    }
}

pub struct WasmerRunner;

impl Runner for WasmerRunner {
    fn run(module: &Module) -> Result<RunResult> {
        let compiler = Singlepass::default();
        let store = Store::new(&Universal::new(compiler).engine());

        let wasmer_module = WasmerModule::new(
            &store,
            module.clone().into_bytes().map_err(anyhow::Error::msg)?,
        )?;

        let ty = MemoryType::new(INITIAL_PAGES, None, false);
        let m = Memory::new(&store, ty).context("memory allocated")?;
        let mem_ptr = m.data_ptr() as usize;
        let mem_size = m.data_size() as usize;
        let memory = Extern::Memory(m);

        let mut exports = Exports::new();
        exports.insert("memory".to_string(), memory.clone());

        let host_function_signature = FunctionType::new(vec![Type::I32], vec![]);
        let host_function = Function::new(&store, &host_function_signature, |_args| {
            Err(RuntimeError::user("out of gas".into()))
        });

        exports.insert(
            SyscallName::SystemBreak.to_str(),
            Extern::Function(host_function),
        );

        let mut imports = ImportObject::new();
        imports.register(MODULE_ENV, exports);

        let instance = match Instance::new(&wasmer_module, &imports) {
            Ok(instance) => instance,
            err @ Err(_) => err?,
        };

        lazy_pages::init_fuzzer_lazy_pages(FuzzerLazyPagesContext {
            instance: Box::new(instance.clone()),
            memory_range: mem_ptr..(mem_ptr + mem_size),
            pages: Default::default(),
            globals_list: globals_list(module),
        });

        instance
            .set_global(GLOBAL_NAME_GAS, PROGRAM_GAS)
            .context("failed to set gas")?;

        let init_fn = instance
            .exports
            .get_function("init")
            .context("init function")?;

        match init_fn.call(&[]) {
            Ok(_) => {}
            Err(e) => {
                if e.message().contains("out of gas") {
                    log::info!("out of gas");
                } else {
                    Err(e)?
                }
            }
        }

        Ok(RunResult {
            gas_global: instance.get_global(GLOBAL_NAME_GAS)?,
            pages: lazy_pages::get_touched_pages(),
            globals: get_globals(&instance, module).context("failed to get globals")?,
        })
    }
}
