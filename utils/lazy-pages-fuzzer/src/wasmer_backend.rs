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
    Exports, Extern, Function, FunctionType, Imports, Instance, Memory, MemoryType,
    Module as WasmerModule, RuntimeError, Singlepass, Store, Type, Value,
};

use crate::{
    globals::{get_globals, globals_list, InstanceAccessGlobal},
    lazy_pages::{self, FuzzerLazyPagesContext},
    RunResult, Runner, INITIAL_PAGES, MODULE_ENV, PROGRAM_GAS,
};

#[derive(Clone)]
struct InstanceBundle {
    instance: Instance,
    store: *mut Store,
}

impl InstanceAccessGlobal for InstanceBundle {
    fn set_global(&self, name: &str, value: i64) -> Result<()> {
        let global = self.instance.exports.get_global(name)?;
        global.set(unsafe { &mut *self.store }, Value::I64(value))?;
        Ok(())
    }

    fn get_global(&self, name: &str) -> Result<i64> {
        let global = self.instance.exports.get_global(name)?;
        let Value::I64(v) = global.get(unsafe { &mut *self.store }) else {
            bail!("global is not an i64")
        };

        Ok(v)
    }
}

pub struct WasmerRunner;

impl Runner for WasmerRunner {
    fn run(module: &Module) -> Result<RunResult> {
        let compiler = Singlepass::default();
        let mut store = Store::new(compiler);

        let wasmer_module = WasmerModule::new(
            &store,
            module.clone().into_bytes().map_err(anyhow::Error::msg)?,
        )?;

        let ty = MemoryType::new(INITIAL_PAGES, None, false);
        let m = Memory::new(&mut store, ty).context("memory allocated")?;
        let mem_view = m.view(&store);
        let mem_ptr = mem_view.data_ptr() as usize;
        let mem_size = mem_view.data_size() as usize;
        let memory = Extern::Memory(m);

        let mut exports = Exports::new();
        exports.insert("memory".to_string(), memory.clone());

        let host_function_signature = FunctionType::new(vec![Type::I32], vec![]);
        let host_function = Function::new(&mut store, &host_function_signature, |_args| {
            Err(RuntimeError::user("out of gas".into()))
        });

        exports.insert(
            SyscallName::SystemBreak.to_str(),
            Extern::Function(host_function),
        );

        let mut imports = Imports::new();
        imports.register_namespace(MODULE_ENV, exports);

        let instance = match Instance::new(&mut store, &wasmer_module, &imports) {
            Ok(instance) => instance,
            err @ Err(_) => err?,
        };

        let instance_bundle = InstanceBundle {
            instance: instance.clone(),
            store: &mut store,
        };

        lazy_pages::init_fuzzer_lazy_pages(FuzzerLazyPagesContext {
            instance: Box::new(instance_bundle.clone()),
            memory_range: mem_ptr..(mem_ptr + mem_size),
            pages: Default::default(),
            globals_list: globals_list(module),
        });

        instance_bundle
            .set_global(GLOBAL_NAME_GAS, PROGRAM_GAS)
            .context("failed to set gas")?;

        let init_fn = instance
            .exports
            .get_function("init")
            .context("init function")?;

        match init_fn.call(&mut store, &[]) {
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
            gas_global: instance_bundle.get_global(GLOBAL_NAME_GAS)?,
            pages: lazy_pages::get_touched_pages(),
            globals: get_globals(&instance_bundle, module).context("failed to get globals")?,
        })
    }
}
