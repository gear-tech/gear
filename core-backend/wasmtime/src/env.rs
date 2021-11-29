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

//! Wasmtime environment for running a module.

use wasmtime::{Extern, Func, Instance, Module, Store, Trap};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;

use crate::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt};
use gear_core::memory::{Memory, PageBuf, PageNumber};

use gear_backend_common::funcs;

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    store: wasmtime::Store,
    ext: LaterExt<E>,
    funcs: BTreeMap<&'static str, Func>,
}

impl<E: Ext + 'static> WasmtimeEnvironment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        let mut result = Self {
            store: Store::default(),
            ext: LaterExt::new(),
            funcs: BTreeMap::new(),
        };

        result.add_func_i32_to_u32("alloc", funcs::alloc);
        result.add_func_i32("free", funcs::free);
        result.add_func_i32("gas", funcs::gas);
        result.add_func_into_i32("gr_block_height", funcs::block_height);
        result.add_func_into_i64("gr_block_timestamp", funcs::block_timestamp);
        result.add_func_to_i32("gr_exit_code", funcs::exit_code);
        result.add_func_into_i64("gr_gas_available", funcs::gas_available);
        result.add_func_i32_i32("gr_debug", funcs::debug);
        result.add_func_i32("gr_msg_id", funcs::msg_id);
        result.add_func_i32_i32_i32("gr_read", funcs::read);
        result.add_func_i32_i32_i64_i32_i32("gr_reply", funcs::reply);
        result.add_func_i32_i64_i32("gr_reply_commit", funcs::reply_commit);
        result.add_func_i32_i32("gr_reply_push", funcs::reply_push);
        result.add_func_i32("gr_reply_to", funcs::reply_to);
        result.add_func_i32_i32_i32_i64_i32_i32("gr_send", funcs::send);
        result.add_func_i32_i32_i32_i64_i32("gr_send_commit", funcs::send_commit);
        result.add_func_to_i32("gr_send_init", funcs::send_init);
        result.add_func_i32_i32_i32("gr_send_push", funcs::send_push);
        result.add_func_into_i32("gr_size", funcs::size);
        result.add_func_i32("gr_source", funcs::source);
        result.add_func_i32("gr_value", funcs::value);
        result.add_func("gr_wait", funcs::wait);
        result.add_func_i32("gr_wake", funcs::wake);

        result
    }

    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    pub fn setup_and_run_inner(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E) {
        let module = Module::new(self.store.engine(), binary).expect("Error creating module");

        self.ext.set(ext);

        let result = self.run_inner(module, memory_pages, memory, move |instance| {
            let result = instance
                .get_func(entry_point)
                .ok_or_else(|| {
                    anyhow::format_err!("failed to find `{}` function export", entry_point)
                })
                .and_then(|entry_func| entry_func.call(&[]))
                .map(|_| ());
            if let Err(e) = &result {
                if let Some(trap) = e.downcast_ref::<Trap>() {
                    if funcs::is_exit_trap(&trap.to_string()) {
                        // We don't propagate a trap when exit
                        return Ok(());
                    }
                }
            }
            result
        });

        let ext = self.ext.unset();

        (result, ext)
    }

    /// Create memory inside this environment.
    pub fn create_memory_inner(&self, total_pages: u32) -> MemoryWrap {
        MemoryWrap::new(
            wasmtime::Memory::new(
                &self.store,
                wasmtime::MemoryType::new(wasmtime::Limits::at_least(total_pages)),
            )
            .expect("Create env memory fail"),
        )
    }

    fn run_inner(
        &mut self,
        module: Module,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        func: impl FnOnce(Instance) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let mut imports = module
            .imports()
            .map(|import| {
                if import.module() != "env" {
                    Err(anyhow::anyhow!("Non-env imports are not supported"))
                } else {
                    Ok((import.name(), Option::<Extern>::None))
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        for (ref import_name, ref mut ext) in imports.iter_mut() {
            if let Some(name) = import_name {
                *ext = match *name {
                    "memory" => {
                        let mem = match memory.as_any().downcast_ref::<wasmtime::Memory>() {
                            Some(mem) => mem,
                            None => panic!("Memory is not wasmtime::Memory"),
                        };
                        Some(wasmtime::Extern::Memory(Clone::clone(mem)))
                    }
                    key if self.funcs.contains_key(key) => Some(self.funcs[key].clone().into()),
                    _ => continue,
                }
            }
        }

        let externs = imports
            .into_iter()
            .map(|(_, host_function)| {
                host_function.ok_or_else(|| anyhow::anyhow!("Missing import"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let instance = Instance::new(&self.store, &module, &externs)?;

        // Set module memory.
        memory
            .set_pages(memory_pages)
            .map_err(|e| anyhow::anyhow!("Can't set module memory: {:?}", e))?;

        func(instance)
    }

    fn add_func<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn() -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap0(func(self.ext.clone()))),
        );
    }

    fn add_func_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap1(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap2(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap3(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i64_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i64, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap3(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32_i64_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i32, i64, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap5(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i64_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i64, i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap5(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_i32_i32_i64_i32_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32, i32, i32, i64, i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap6(func(self.ext.clone()))),
        );
    }

    fn add_func_i32_to_u32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn(i32) -> Result<u32, &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap1(func(self.ext.clone()))),
        );
    }

    fn add_func_into_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn() -> i32,
    {
        self.funcs
            .insert(key, Func::wrap(&self.store, func(self.ext.clone())));
    }

    fn add_func_to_i32<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn() -> Result<i32, &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap0(func(self.ext.clone()))),
        );
    }

    fn add_func_into_i64<F>(&mut self, key: &'static str, func: fn(LaterExt<E>) -> F)
    where
        F: 'static + Fn() -> i64,
    {
        self.funcs
            .insert(key, Func::wrap(&self.store, func(self.ext.clone())));
    }

    fn wrap0<R>(func: impl Fn() -> Result<R, &'static str>) -> impl Fn() -> Result<R, Trap> {
        move || func().map_err(Trap::new)
    }

    fn wrap1<T, R>(func: impl Fn(T) -> Result<R, &'static str>) -> impl Fn(T) -> Result<R, Trap> {
        move |a| func(a).map_err(Trap::new)
    }

    fn wrap2<T0, T1, R>(
        func: impl Fn(T0, T1) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1) -> Result<R, Trap> {
        move |a, b| func(a, b).map_err(Trap::new)
    }

    fn wrap3<T0, T1, T2, R>(
        func: impl Fn(T0, T1, T2) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2) -> Result<R, Trap> {
        move |a, b, c| func(a, b, c).map_err(Trap::new)
    }

    fn wrap5<T0, T1, T2, T3, T4, R>(
        func: impl Fn(T0, T1, T2, T3, T4) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2, T3, T4) -> Result<R, Trap> {
        move |a, b, c, d, e| func(a, b, c, d, e).map_err(Trap::new)
    }

    fn wrap6<T0, T1, T2, T3, T4, T5, R>(
        func: impl Fn(T0, T1, T2, T3, T4, T5) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2, T3, T4, T5) -> Result<R, Trap> {
        move |a, b, c, d, e, f| func(a, b, c, d, e, f).map_err(Trap::new)
    }
}

impl<E: Ext + 'static> Default for WasmtimeEnvironment<E> {
    /// Create a default environment.
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Ext> gear_backend_common::Environment<E> for WasmtimeEnvironment<E> {
    fn setup_and_run(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn gear_core::memory::Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E) {
        self.setup_and_run_inner(ext, binary, memory_pages, memory, entry_point)
    }

    fn create_memory(&self, total_pages: u32) -> Box<dyn Memory> {
        Box::new(self.create_memory_inner(total_pages))
    }
}
