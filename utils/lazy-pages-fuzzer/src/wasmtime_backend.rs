// This file is part of Gear.

// Copyright (C) 2021-2025 Gear Technologies Inc.
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

use crate::{
    INITIAL_PAGES, MODULE_ENV, PROGRAM_GAS, RunResult, Runner,
    globals::{InstanceAccessGlobal, get_globals, globals_list},
    lazy_pages::{self, FuzzerLazyPagesContext},
};
use anyhow::{Context, Result, bail};
use gear_wasm_gen::SyscallName;
use gear_wasm_instrument::{GLOBAL_NAME_GAS, Module};
use wasmtime::{
    Config, Engine, Extern, Func, Instance, Linker, Memory, MemoryType, Module as WasmtimeModule,
    Store, Strategy, Val,
};

#[derive(Clone)]
struct InstanceBundle {
    instance: Instance,
    // NOTE: Due to the implementation of lazy pages, which need to access the Store to retrieve globals,
    // we have to use a second mutable reference to the Store in the form of a raw pointer
    // to use it within the lazy pages' signal handler context.
    //
    // We consider it relatively safe because we rely on the fact that during an external function call,
    // Wasmtime does not access globals mutably, allowing us to access them mutably from the lazy pages' signal handler.
    store: *mut Store<()>,
}

impl InstanceAccessGlobal for InstanceBundle {
    fn set_global(&self, name: &str, value: i64) -> Result<()> {
        let global = self
            .instance
            .get_global(unsafe { &mut *self.store }, name)
            .context("missing global")?;
        global.set(unsafe { &mut *self.store }, Val::I64(value))?;
        Ok(())
    }

    fn get_global(&self, name: &str) -> Result<i64> {
        let global = self
            .instance
            .get_global(unsafe { &mut *self.store }, name)
            .context("missing global")?;
        let Val::I64(v) = global.get(unsafe { &mut *self.store }) else {
            bail!("global is not an i64")
        };

        Ok(v)
    }
}

pub struct WasmtimeRunner;

impl Runner for WasmtimeRunner {
    fn run(module: &Module) -> Result<RunResult> {
        let mut config = Config::new();
        config.strategy(Strategy::Winch).macos_use_mach_ports(false);
        let engine = Engine::new(&config).context("failed to create engine")?;
        let mut store = Store::new(&engine, ());

        let wasmtime_module = WasmtimeModule::new(
            store.engine(),
            module.serialize().map_err(anyhow::Error::msg)?,
        )?;

        let ty = MemoryType::new(INITIAL_PAGES, None);
        let m = Memory::new(&mut store, ty).context("memory allocated")?;
        let mem_ptr = m.data_ptr(&store) as usize;
        let mem_size = m.data_size(&store);
        let memory = Extern::Memory(m);

        let mut linker = Linker::new(&engine);
        linker
            .define(&store, MODULE_ENV, "memory", memory.clone())
            .context("failed to define memory")?;

        let host_function = Func::wrap(&mut store, |_arg: i32| {
            Err::<(), _>(anyhow::anyhow!("out of gas"))
        });

        linker
            .define(
                &store,
                "env",
                SyscallName::SystemBreak.to_str(),
                host_function,
            )
            .context("failed to define func")?;

        let instance = linker.instantiate(&mut store, &wasmtime_module)?;

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
            .get_func(&mut store, "init")
            .context("init function")?;

        match init_fn.call(&mut store, &[], &mut []) {
            Ok(_) => {}
            Err(e) => {
                if e.to_string().contains("out of gas") {
                    log::debug!("out of gas");
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
