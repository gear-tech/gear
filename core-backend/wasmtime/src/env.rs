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

use alloc::{boxed::Box, collections::BTreeMap, format, string::ToString, vec::Vec};
use gear_backend_common::{
    funcs as common_funcs, BackendError, BackendReport, Environment, ExtInfo, IntoExtInfo,
    TerminationReason,
};
use gear_core::{
    env::{Ext, LaterExt},
    gas::GasAmount,
    memory::{Error, PageBuf, PageNumber},
};
use wasmtime::{
    Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store, Trap,
};

/// Complitelly same as LaterExt, but with Sync + Send implementations,
/// which is needed only for wasmtime restrictions and never used actually.
/// TODO: see https://github.com/gear-tech/gear/issues/763
pub struct SyncLaterExt<E: Ext>(LaterExt<E>);

impl<E: Ext> Default for SyncLaterExt<E> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<E: Ext> Clone for SyncLaterExt<E> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<E: Ext> SyncLaterExt<E> {
    /// Set ext
    pub fn set(&mut self, e: E) {
        self.0.set(e)
    }

    /// Call fn with inner ext
    pub fn with<R>(&self, f: impl FnOnce(&mut E) -> R) -> Result<R, &'static str> {
        self.0.with(f)
    }

    /// Call fn with inner ext
    pub fn with_fallible<R>(
        &self,
        f: impl FnOnce(&mut E) -> Result<R, &'static str>,
    ) -> Result<R, &'static str> {
        self.0.with_fallible(f)
    }

    /// Unset inner ext
    pub fn unset(&mut self) -> E {
        self.0.unset()
    }
}

unsafe impl<E: Ext> Sync for SyncLaterExt<E> {}
unsafe impl<E: Ext> Send for SyncLaterExt<E> {}

/// Data type in wasmtime store. Not used actually in our case.
pub struct StoreData;

/// Environment to run one module at a time providing Ext.
pub struct WasmtimeEnvironment<E: Ext + 'static> {
    store: Store<StoreData>,
    ext: SyncLaterExt<E>,
    memory: WasmtimeMemory,
    instance: Instance,
}

fn set_pages(
    mut store: &mut Store<StoreData>,
    memory: &mut WasmtimeMemory,
    pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
) -> Result<(), Error> {
    for (num, buf) in pages {
        if memory.size(&mut store) <= num.raw() as u64 {
            return Err(Error::MemoryAccessError);
        }
        if let Some(buf) = buf {
            memory
                .write(&mut store, num.offset(), &buf[..])
                .map_err(|_| Error::MemoryAccessError)?;
        }
    }
    Ok(())
}

impl<E: Ext + IntoExtInfo> Environment<E> for WasmtimeEnvironment<E> {
    fn new(
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Option<Box<PageBuf>>>,
        mem_size: u32,
    ) -> Result<Self, BackendError<'static>> {
        let mut later_ext = SyncLaterExt::default();
        later_ext.set(ext);

        let engine = Engine::default();
        let mut store = Store::<StoreData>::new(&engine, StoreData);

        // Creates new wasm memory
        let mut memory =
            WasmtimeMemory::new(&mut store, MemoryType::new(mem_size, None)).map_err(|e| {
                BackendError {
                    reason: "Create env memory failed",
                    description: Some(e.to_string().into()),
                    gas_amount: later_ext.unset().into_gas_amount(),
                }
            })?;

        /// Make import funcs
        use crate::funcs::FuncsHandler as funcs;
        let mut funcs = BTreeMap::<&'static str, Func>::new();
        funcs.insert("alloc", funcs::alloc(later_ext.clone(), &mut store, memory));
        funcs.insert("free", funcs::free(later_ext.clone(), &mut store));
        funcs.insert("gas", funcs::gas(later_ext.clone(), &mut store));
        funcs.insert(
            "gr_block_height",
            funcs::block_height(later_ext.clone(), &mut store),
        );
        funcs.insert(
            "gr_block_timestamp",
            funcs::block_timestamp(later_ext.clone(), &mut store),
        );
        funcs.insert(
            "gr_create_program_wgas",
            funcs::create_program_wgas(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_exit_code",
            funcs::exit_code(later_ext.clone(), &mut store),
        );
        funcs.insert(
            "gr_gas_available",
            funcs::gas_available(later_ext.clone(), &mut store),
        );
        funcs.insert(
            "gr_debug",
            funcs::debug(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_exit",
            funcs::exit(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_origin",
            funcs::origin(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_msg_id",
            funcs::msg_id(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_program_id",
            funcs::program_id(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_read",
            funcs::read(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_reply",
            funcs::reply(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_reply_commit",
            funcs::reply_commit(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_reply_push",
            funcs::reply_push(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_reply_to",
            funcs::reply_to(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_send_wgas",
            funcs::send_wgas(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_send",
            funcs::send(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_send_commit_wgas",
            funcs::send_commit_wgas(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_send_commit",
            funcs::send_commit(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_send_init",
            funcs::send_init(later_ext.clone(), &mut store),
        );
        funcs.insert(
            "gr_send_push",
            funcs::send_push(later_ext.clone(), &mut store, memory),
        );
        funcs.insert("gr_size", funcs::size(later_ext.clone(), &mut store));
        funcs.insert(
            "gr_source",
            funcs::source(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_value",
            funcs::value(later_ext.clone(), &mut store, memory),
        );
        funcs.insert(
            "gr_value_available",
            funcs::value_available(later_ext.clone(), &mut store, memory),
        );
        funcs.insert("gr_leave", funcs::leave(later_ext.clone(), &mut store));
        funcs.insert("gr_wait", funcs::wait(later_ext.clone(), &mut store));
        funcs.insert(
            "gr_wake",
            funcs::wake(later_ext.clone(), &mut store, memory),
        );

        let module = Module::new(&engine, binary).map_err(|e| BackendError {
            reason: "Unable to create module",
            description: Some(e.to_string().into()),
            gas_amount: later_ext.unset().into_gas_amount(),
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
                        gas_amount: later_ext.unset().into_gas_amount(),
                    })
                } else {
                    Ok((import.name(), Option::<Extern>::None))
                }
            })
            .collect::<Result<Vec<_>, BackendError>>()?;

        for (import_name, ref mut ext) in imports.iter_mut() {
            if let Some(name) = import_name {
                *ext = match *name {
                    "memory" => Some(Extern::Memory(memory)),
                    key if funcs.contains_key(key) => Some(funcs[key].into()),
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
                    gas_amount: later_ext.unset().into_gas_amount(),
                })
            })
            .collect::<Result<Vec<_>, BackendError>>()?;

        let instance = Instance::new(&mut store, &module, &externs).map_err(|e| BackendError {
            reason: "Unable to create instance",
            description: Some(e.to_string().into()),
            gas_amount: later_ext.unset().into_gas_amount(),
        })?;

        // Set module memory data
        set_pages(&mut store, &mut memory, memory_pages).map_err(|e| BackendError {
            reason: "Unable to set module memory data",
            description: Some(format!("{:?}", e).into()),
            gas_amount: later_ext.unset().into_gas_amount(),
        })?;

        Ok(WasmtimeEnvironment {
            store,
            ext: later_ext,
            memory,
            instance,
        })
    }

    fn get_stack_mem_end(&mut self) -> Option<i32> {
        // `__gear_stack_end` export is inserted in wasm-proc or wasm-builder
        let global = self
            .instance
            .get_global(&mut self.store, "__gear_stack_end")?;
        global.get(&mut self.store).i32()
    }

    fn get_wasm_memory_begin_addr(&mut self) -> usize {
        self.memory.data_ptr(&mut self.store) as usize
    }

    fn execute(&mut self, entry_point: &str) -> Result<BackendReport, BackendError> {
        let func = self.instance.get_func(&mut self.store, entry_point);

        let entry_func = if let Some(f) = func {
            // Entry function found
            f
        } else {
            // Entry function not found, so we mean this as empty function
            return Ok(BackendReport {
                termination: TerminationReason::Success,
                wasm_memory_addr: self.memory.data_ptr(&self.store) as usize,
                info: self
                    .ext
                    .unset()
                    .into_ext_info(|offset: usize, buffer: &mut [u8]| {
                        self.memory
                            .read(&mut self.store, offset, buffer)
                            .expect("Must can be read");
                    }),
            });
        };

        let res = entry_func.call(&mut self.store, &[], &mut []);

        let info: ExtInfo = self
            .ext
            .unset()
            .into_ext_info(|offset: usize, buffer: &mut [u8]| {
                self.memory
                    .read(&mut self.store, offset, buffer)
                    .expect("Must can be read");
            });

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

        let wasm_memory_addr = self.memory.data_ptr(&self.store) as usize;

        Ok(BackendReport {
            termination,
            wasm_memory_addr,
            info,
        })
    }

    fn drop_env(&mut self) -> GasAmount {
        self.ext.unset().into_gas_amount()
    }
}
