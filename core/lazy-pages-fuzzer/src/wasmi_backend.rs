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
use anyhow::{Context, anyhow, bail};
use error::CustomHostError;
use gear_wasm_gen::SyscallName;
use gear_wasm_instrument::{GLOBAL_NAME_GAS, Module};
use region::{Allocation, Protection};
use std::slice;
use wasmi::{
    Caller, Config, Engine, Error, Instance, Linker, Memory, MemoryType, Module as WasmiModule,
    StackLimits, Store, Val, core::UntypedVal,
};

mod error;

#[derive(Clone)]
struct InstanceBundle {
    instance: Instance,
    // NOTE: Due to the implementation of lazy pages, which need to access the Store to retrieve globals,
    // we have to use a second mutable reference to the Store in the form of a raw pointer
    // to use it within the lazy pages' signal handler context.
    //
    // We consider it relatively safe because we rely on the fact that during an external function call,
    // Wasmi does not access globals mutably, allowing us to access them mutably from the lazy pages' signal handler.
    store: *mut Store<()>,
}

impl InstanceAccessGlobal for InstanceBundle {
    fn set_global(&self, name: &str, value: i64) -> anyhow::Result<()> {
        let global = self
            .instance
            .get_global(unsafe { &*self.store }, name)
            .ok_or_else(|| anyhow!("failed to get global {name}"))?;
        global.set(unsafe { &mut *self.store }, Val::I64(value))?;
        Ok(())
    }

    fn get_global(&self, name: &str) -> anyhow::Result<i64> {
        let global = self
            .instance
            .get_global(unsafe { &*self.store }, name)
            .ok_or_else(|| anyhow!("failed to get global {name}"))?;
        let Val::I64(v) = global.get(unsafe { &mut *self.store }) else {
            bail!("global {name} is not an i64")
        };

        Ok(v)
    }
}

fn config() -> Config {
    let register_len = size_of::<UntypedVal>();

    const DEFAULT_MIN_VALUE_STACK_HEIGHT: usize = 1024;
    // Fuzzer requires bigger stack size
    const DEFAULT_MAX_VALUE_STACK_HEIGHT: usize = 1024 * DEFAULT_MIN_VALUE_STACK_HEIGHT * 16;
    const DEFAULT_MAX_RECURSION_DEPTH: usize = 16384;

    let mut config = Config::default();
    config.set_stack_limits(
        StackLimits::new(
            DEFAULT_MIN_VALUE_STACK_HEIGHT / register_len,
            DEFAULT_MAX_VALUE_STACK_HEIGHT / register_len,
            DEFAULT_MAX_RECURSION_DEPTH,
        )
        .expect("infallible"),
    );

    config
}

fn memory(store: &mut Store<()>) -> anyhow::Result<(Memory, Allocation)> {
    let mut alloc = region::alloc(u32::MAX as usize, Protection::READ_WRITE)
        .unwrap_or_else(|err| unreachable!("Failed to allocate memory: {err}"));
    // # Safety:
    //
    // `wasmi::Memory::new_static()` requires static lifetime so we convert our buffer to it
    // but actual lifetime of the buffer is lifetime of `wasmi::Store` itself,
    // because the store might hold reference to the memory.
    let memref =
        unsafe { slice::from_raw_parts_mut::<'static, u8>(alloc.as_mut_ptr(), alloc.len()) };
    let ty = MemoryType::new(INITIAL_PAGES, None).context("failed to create memory type")?;
    let memref = Memory::new_static(store, ty, memref).context("failed to create memory")?;

    Ok((memref, alloc))
}

pub struct WasmiRunner;

impl Runner for WasmiRunner {
    fn run(module: &Module) -> anyhow::Result<RunResult> {
        let engine = Engine::new(&config());

        let wasmi_module =
            WasmiModule::new(&engine, &module.serialize().map_err(anyhow::Error::msg)?)
                .context("failed to load wasm")?;

        let mut store = Store::new(&engine, ());

        // NOTE: alloc should be dropped after exit of this function's scope
        let (memory, _alloc) = memory(&mut store)?;
        let mem_ptr = memory.data_ptr(&store) as usize;
        let mem_size = memory.data_size(&store);

        let mut linker: Linker<()> = <Linker<()>>::new(&engine);
        linker
            .func_wrap(
                MODULE_ENV,
                SyscallName::SystemBreak.to_str(),
                |_caller: Caller<()>, _param: i32| -> Result<(), Error> {
                    Err(Error::host(CustomHostError::from("out of gas")))
                },
            )
            .context("failed to define host function")?;

        linker
            .define(MODULE_ENV, "memory", memory)
            .context("failed to define memory")?;

        let instance = linker
            .instantiate(&mut store, &wasmi_module)
            .context("failed to instantiate wasm module")?
            .ensure_no_start(&mut store)
            .context("failed to ensure no start")?;

        let gas = instance
            .get_global(&store, GLOBAL_NAME_GAS)
            .context("failed to get gas")?;
        gas.set(&mut store, Val::I64(PROGRAM_GAS))
            .context("failed to set gas")?;

        let global_accessor = InstanceBundle {
            instance,
            store: &mut store,
        };

        let init_fn = instance
            .get_func(&store, "init")
            .context("failed to get export fn")?;

        lazy_pages::init_fuzzer_lazy_pages(FuzzerLazyPagesContext {
            instance: Box::new(global_accessor.clone()),
            memory_range: mem_ptr..(mem_ptr + mem_size),
            pages: Default::default(),
            globals_list: globals_list(module),
        });

        if let Err(error) = init_fn.call(&mut store, &[], &mut []) {
            if let Some(custom_error) = error.downcast_ref::<CustomHostError>() {
                log::debug!("{custom_error}");
            } else {
                Err(error)?;
            }
        }

        let result = RunResult {
            gas_global: gas.get(&store).i64().context("failed to get gas global")?,
            pages: lazy_pages::get_touched_pages(),
            globals: get_globals(&global_accessor, module).context("failed to get globals")?,
        };

        Ok(result)
    }
}
