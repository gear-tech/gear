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

use crate::memory::MemoryWrap;
use alloc::{boxed::Box, collections::BTreeMap, format, string::ToString, vec::Vec};
use gear_backend_common::{
    funcs, BackendError, BackendReport, Environment, ExtInfo, TerminationReason,
};
use gear_core::{
    env::{Ext, LaterExt},
    memory::{Memory, PageBuf, PageNumber},
};
use wasmtime::{
    Extern, Func, Instance, Limits, Memory as WasmtimeMemory, MemoryType, Module, Store, Trap,
};

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    store: wasmtime::Store,
    ext: LaterExt<E>,
    funcs: BTreeMap<&'static str, Func>,
    instance: Option<Instance>,
}

impl<E: Ext + 'static> WasmtimeEnvironment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        let mut result = Self {
            store: Store::default(),
            ext: Default::default(),
            funcs: BTreeMap::new(),
            instance: None,
        };

        result.add_func_i32_to_u32("alloc", funcs::alloc);
        result.add_func_i32("free", funcs::free);
        result.add_func_i32("gas", funcs::gas);
        result.add_func_into_i32("gr_block_height", funcs::block_height);
        result.add_func_into_i64("gr_block_timestamp", funcs::block_timestamp);
        result.add_func_to_i32("gr_exit_code", funcs::exit_code);
        result.add_func_into_i64("gr_gas_available", funcs::gas_available);
        result.add_func_i32_i32("gr_debug", funcs::debug);
        result.add_func_i32("gr_exit", funcs::exit);
        result.add_func_i32("gr_msg_id", funcs::msg_id);
        result.add_func_i32("gr_program_id", funcs::program_id);
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
        result.add_func_i32("gr_value_available", funcs::value_available);
        result.add_func("gr_leave", funcs::leave);
        result.add_func("gr_wait", funcs::wait);
        result.add_func_i32("gr_wake", funcs::wake);
        result.add_func_i32_i32_i32_i32_i32_i64_i32_i32("gr_create_program", funcs::create_program);

        result
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

    fn add_func_i32_i32_i32_i32_i32_i64_i32_i32<F>(
        &mut self,
        key: &'static str,
        func: fn(LaterExt<E>) -> F,
    ) where
        F: 'static + Fn(i32, i32, i32, i32, i32, i64, i32, i32) -> Result<(), &'static str>,
    {
        self.funcs.insert(
            key,
            Func::wrap(&self.store, Self::wrap8(func(self.ext.clone()))),
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

    fn wrap8<T0, T1, T2, T3, T4, T5, T6, T7, R>(
        func: impl Fn(T0, T1, T2, T3, T4, T5, T6, T7) -> Result<R, &'static str>,
    ) -> impl Fn(T0, T1, T2, T3, T4, T5, T6, T7) -> Result<R, Trap> {
        move |a, b, c, d, e, f, g, h| func(a, b, c, d, e, f, g, h).map_err(Trap::new)
    }
}

impl<E: Ext + 'static> Default for WasmtimeEnvironment<E> {
    /// Create a default environment.
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Ext + Into<ExtInfo>> Environment<E> for WasmtimeEnvironment<E> {
    fn setup(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        memory: &dyn Memory,
    ) -> Result<(), BackendError<'static>> {
        self.ext.set(ext);

        let module = Module::new(self.store.engine(), binary).map_err(|e| BackendError {
            reason: "Unable to create module",
            description: Some(e.to_string().into()),
            gas_amount: self.ext.unset().into().gas_amount,
        })?;

        let mut imports = module
            .imports()
            .map(|import| {
                if import.module() != "env" {
                    Err(BackendError {
                        reason: "Non-env imports are not supported",
                        description: import
                            .name()
                            .map(|v| format!("Function {:?} is not env", v).into()),
                        gas_amount: self.ext.unset().into().gas_amount,
                    })
                } else {
                    Ok((import.name(), Option::<Extern>::None))
                }
            })
            .collect::<Result<Vec<_>, BackendError>>()?;

        for (import_name, ref mut ext) in imports.iter_mut() {
            if let Some(name) = import_name {
                *ext = match *name {
                    "memory" => match memory.as_any().downcast_ref::<WasmtimeMemory>() {
                        Some(mem) => Some(Extern::Memory(mem.clone())),
                        _ => {
                            return Err(BackendError {
                                reason: "Memory is not wasmtime::Memory",
                                description: None,
                                gas_amount: self.ext.unset().into().gas_amount,
                            })
                        }
                    },
                    key if self.funcs.contains_key(key) => Some(self.funcs[key].clone().into()),
                    _ => continue,
                }
            }
        }

        let externs = imports
            .into_iter()
            .map(|(name, host_function)| {
                host_function.ok_or_else(|| BackendError {
                    reason: "Missing import",
                    description: name
                        .map(|v| format!("Function {:?} definition wasn't found", v).into()),
                    gas_amount: self.ext.unset().into().gas_amount,
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;

        let instance = Instance::new(&self.store, &module, &externs).map_err(|e| BackendError {
            reason: "Unable to create instance",
            description: Some(e.to_string().into()),
            gas_amount: self.ext.unset().into().gas_amount,
        })?;

        // Set module memory.
        memory.set_pages(memory_pages).map_err(|e| BackendError {
            reason: "Unable to set module memory",
            description: Some(format!("{:?}", e).into()),
            gas_amount: self.ext.unset().into().gas_amount,
        })?;

        self.instance.replace(instance);

        Ok(())
    }

    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError> {
        let instance = self.instance.as_mut().expect("Must have instance");

        let entry_func = match instance.get_func(entry_point) {
            // Entry function found
            Some(f) => f,
            // Entry function not found, so we mean this as empty function
            _ => {
                return Ok(BackendReport {
                    termination: TerminationReason::Success,
                    info: self.ext.unset().into(),
                })
            }
        };

        let res = entry_func.call(&[]);

        let info: ExtInfo = self.ext.unset().into();

        let termination = if let Err(e) = &res {
            let reason = if let Some(trap) = e.downcast_ref::<Trap>() {
                let trap = trap.to_string();

                if let Some(value_dest) = info.exit_argument {
                    Some(TerminationReason::Exit(value_dest))
                } else if funcs::is_wait_trap(&trap) {
                    Some(TerminationReason::Wait)
                } else if funcs::is_leave_trap(&trap) {
                    Some(TerminationReason::Leave)
                } else {
                    None
                }
            } else {
                None
            };

            reason.unwrap_or_else(|| TerminationReason::Trap {
                explanation: info.trap_explanation,
                description: Some(e.to_string().into()),
            })
        } else {
            TerminationReason::Success
        };

        Ok(BackendReport { termination, info })
    }

    fn create_memory(&self, total_pages: u32) -> Result<Box<dyn Memory>, &'static str> {
        Ok(Box::new(MemoryWrap::new(
            WasmtimeMemory::new(&self.store, MemoryType::new(Limits::at_least(total_pages)))
                .map_err(|_| "Create env memory fail")?,
        )))
    }
}
