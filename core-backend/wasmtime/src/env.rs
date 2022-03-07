// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use alloc::rc::Rc;

use crate::memory::MemoryWrap;
use alloc::{boxed::Box, collections::BTreeMap, format, string::ToString, vec::Vec};
use gear_backend_common::{
    funcs as common_funcs, BackendError, BackendReport, Environment, ExtInfo, TerminationReason,
};
use gear_core::{
    env::{Ext, LaterExt},
    memory::{Memory, PageBuf, PageNumber},
};
use wasmtime::{
    Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store, Trap,
};

pub struct LaterStore<T>(Rc<Store<T>>);

impl<T> Clone for LaterStore<T> {
    fn clone(&self) -> Self {
        LaterStore(Rc::clone(&self.0))
    }
}

impl<T: Default> LaterStore<T> {
    pub fn new(eng: &Engine) -> Self {
        LaterStore(Rc::from(Store::new(eng, T::default())))
    }
    /// In order to be able borrow mutable reference many times we need
    /// to make it in unsafe manner.
    /// Wasmtime store object must be mut borrowed to execute instance,
    /// but also we must mut borrow it in memory sys-calls: alloc/free/...
    /// But memory syscalls called in the same time with instance execution,
    /// so there is no ways to avoid twice mut borrowing.
    pub fn get_mut_ref(&mut self) -> &mut Store<T> {
        unsafe {
            let r = self.0.as_ref();
            let ptr = r as *const Store<T> as *mut Store<T>;
            ptr.as_mut().expect("ptr must be here")
        }
    }
}

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    store: LaterStore<()>,
    ext: LaterExt<E>,
    funcs: BTreeMap<&'static str, Func>,
    instance: Option<Instance>,
    engine: Engine,
}

impl<E: Ext + 'static> Default for WasmtimeEnvironment<E> {
    /// Create a default environment.
    fn default() -> Self {
        let engine = Engine::default();
        let store = LaterStore::new(&engine);
        Self {
            engine,
            store,
            ext: Default::default(),
            funcs: BTreeMap::new(),
            instance: None,
        }
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

        use crate::funcs::FuncsHandler as funcs;
        let tmp_ext = self.ext.clone();
        let mut tmp_store = self.store.clone();
        let store = LaterStore::<()>::get_mut_ref(&mut tmp_store);
        self.funcs
            .insert("alloc", funcs::alloc(tmp_ext.clone(), store));
        self.funcs
            .insert("free", funcs::free(tmp_ext.clone(), store));
        self.funcs.insert("gas", funcs::gas(tmp_ext.clone(), store));
        self.funcs.insert(
            "gr_block_height",
            funcs::block_height(tmp_ext.clone(), store),
        );
        self.funcs.insert(
            "gr_block_timestamp",
            funcs::block_timestamp(tmp_ext.clone(), store),
        );
        self.funcs.insert(
            "gr_create_program_wgas",
            funcs::create_program_wgas(tmp_ext.clone(), store),
        );
        self.funcs
            .insert("gr_exit_code", funcs::exit_code(tmp_ext.clone(), store));
        self.funcs.insert(
            "gr_gas_available",
            funcs::gas_available(tmp_ext.clone(), store),
        );
        self.funcs
            .insert("gr_debug", funcs::debug(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_exit", funcs::exit(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_origin", funcs::origin(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_msg_id", funcs::msg_id(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_program_id", funcs::program_id(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_read", funcs::read(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_reply", funcs::reply(tmp_ext.clone(), store));
        self.funcs.insert(
            "gr_reply_commit",
            funcs::reply_commit(tmp_ext.clone(), store),
        );
        self.funcs
            .insert("gr_reply_push", funcs::reply_push(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_reply_to", funcs::reply_to(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_send_wgas", funcs::send_wgas(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_send", funcs::send(tmp_ext.clone(), store));
        self.funcs.insert(
            "gr_send_commit_wgas",
            funcs::send_commit_wgas(tmp_ext.clone(), store),
        );
        self.funcs
            .insert("gr_send_commit", funcs::send_commit(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_send_init", funcs::send_init(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_send_push", funcs::send_push(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_size", funcs::size(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_source", funcs::source(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_value", funcs::value(tmp_ext.clone(), store));
        self.funcs.insert(
            "gr_value_available",
            funcs::value_available(tmp_ext.clone(), store),
        );
        self.funcs
            .insert("gr_leave", funcs::leave(tmp_ext.clone(), store));
        self.funcs
            .insert("gr_wait", funcs::wait(tmp_ext.clone(), store));
        self.funcs.insert("gr_wake", funcs::wake(tmp_ext, store));

        let module = Module::new(&self.engine, binary).map_err(|e| BackendError {
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
                        Some(mem) => Some(Extern::Memory(*mem)),
                        _ => {
                            return Err(BackendError {
                                reason: "Memory is not wasmtime::Memory",
                                description: None,
                                gas_amount: self.ext.unset().into().gas_amount,
                            })
                        }
                    },
                    key if self.funcs.contains_key(key) => Some(self.funcs[key].into()),
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

        let instance = Instance::new(store, &module, &externs).map_err(|e| BackendError {
            reason: "Unable to create instance",
            description: Some(e.to_string().into()),
            gas_amount: self.ext.unset().into().gas_amount,
        })?;

        // Set module memory data
        memory.set_pages(memory_pages).map_err(|e| BackendError {
            reason: "Unable to set module memory",
            description: Some(format!("{:?}", e).into()),
            gas_amount: self.ext.unset().into().gas_amount,
        })?;

        self.instance.replace(instance);

        Ok(())
    }

    fn get_stack_mem_end(&self) -> Option<i32> {
        // `__gear_stack_end` export is inserted in wasm-proc or wasm-builder
        let instance = self.instance.as_ref().expect("Must have instance");
        let global = instance.get_global(self.store.clone().get_mut_ref(), "__gear_stack_end")?;
        global.get(self.store.clone().get_mut_ref()).i32()
    }

    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError> {
        let instance = self.instance.as_mut().expect("Must have instance");
        let func = instance.get_func(self.store.clone().get_mut_ref(), entry_point);
        let entry_func = if let Some(f) = func {
            // Entry function found
            f
        } else {
            // Entry function not found, so we mean this as empty function
            return Ok(BackendReport {
                termination: TerminationReason::Success,
                info: self.ext.unset().into(),
            });
        };

        let res = entry_func.call(self.store.clone().get_mut_ref(), &[], &mut []);

        let info: ExtInfo = self.ext.unset().into();

        let termination = if let Err(e) = &res {
            let reason = if let Some(trap) = e.downcast_ref::<Trap>() {
                let trap = trap.to_string();

                if let Some(value_dest) = info.exit_argument {
                    Some(TerminationReason::Exit(value_dest))
                } else if common_funcs::is_wait_trap(&trap) {
                    Some(TerminationReason::Wait)
                } else if common_funcs::is_leave_trap(&trap) {
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
        let memory = WasmtimeMemory::new(
            self.store.clone().get_mut_ref(),
            MemoryType::new(total_pages, None),
        )
        .map_err(|_| "Create env memory fail")?;
        Ok(Box::new(MemoryWrap::new(memory, &self.store)))
    }
}
