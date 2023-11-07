// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! The WASM executor in this module is just for parsing the state types
//! of gear programs, some of the host functions are missing logics that
//! is because they are for the on-chain environment data.

use anyhow::{anyhow, Result};
use wasmi::{AsContextMut, Engine, Extern, Linker, Memory, MemoryType, Module, Store};

const PAGE_STORAGE_PREFIX: [u8; 32] = *b"gcligcligcligcligcligcligcligcli";

/// HostState for the WASM executor
#[derive(Default)]
pub struct HostState {
    /// Message buffer in host state.
    pub msg: Vec<u8>,
}

/// Call `metadata` method in the WASM code.
pub fn call_metadata(wasm: &[u8]) -> Result<Vec<u8>> {
    execute(wasm, "metadata")
}

/// Executes the WASM code.
fn execute(wasm: &[u8], method: &str) -> Result<Vec<u8>> {
    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        PAGE_STORAGE_PREFIX
    ));

    let engine = Engine::default();
    let module = Module::new(&engine, wasm).unwrap();

    let mut store = Store::new(&engine, HostState::default());
    let mut linker = <Linker<HostState>>::new(&engine);
    let memory = Memory::new(
        store.as_context_mut(),
        MemoryType::new(256, None).map_err(|_| anyhow!("failed to create memory type"))?,
    )
    .map_err(|_| anyhow!("failed to create memory"))?;

    // Execution environment
    //
    // TODO: refactor this after #3416.
    {
        let mut env = env::Env {
            linker: &mut linker,
            store: &mut store,
            memory,
        };

        for import in module.imports() {
            env.define(import.module(), import.name())?;
        }
    }

    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)?;

    let metadata = instance
        .get_export(&store, method)
        .and_then(Extern::into_func)
        .ok_or_else(|| anyhow!("could not find function \"{}\"", method))?
        .typed::<(), ()>(&mut store)?;

    metadata.call(&mut store, ())?;
    Ok(store.data().msg.clone())
}

mod env {
    use super::HostState;
    use anyhow::{anyhow, Result};
    use wasmi::{
        core::{Pages, Trap, TrapCode},
        AsContext, AsContextMut, Caller, Extern, Func, Linker, Memory, Store,
    };

    /// Environment for the wasm execution.
    pub struct Env<'e> {
        pub linker: &'e mut Linker<HostState>,
        pub store: &'e mut Store<HostState>,
        pub memory: Memory,
    }

    impl Env<'_> {
        /// Define function in the environment.
        pub fn define(&mut self, module: &str, name: &str) -> Result<()> {
            if module != "env" {
                return Err(anyhow!("module \"{}\" not found", module));
            }

            let memory = self.memory;
            let store = &mut self.store;

            let external = match name {
                "memory" => Extern::Memory(memory),
                "alloc" => alloc(self.store, memory),
                "free" => free(self.store),
                "gr_oom_panic" => gr_oom_panic(store),
                "gr_read" => gr_read(store, memory),
                "gr_reply" => gr_reply(store, memory),
                "gr_panic" => gr_panic(store, memory),
                "gr_size" => gr_size(store, memory),
                "gr_out_of_gas" => gr_out_of_gas(store),
                _ => return Err(anyhow!("export \"{}\" not found in env", name,)),
            };

            self.linker.define(module, name, external)?;

            Ok(())
        }
    }

    fn alloc(store: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            store,
            move |mut caller: Caller<'_, HostState>, pages: u32| {
                memory
                    .clone()
                    .grow(
                        caller.as_context_mut(),
                        Pages::new(pages).unwrap_or_default(),
                    )
                    .map_or_else(
                        |err| {
                            log::error!("{err:?}");
                            u32::MAX as i32
                        },
                        |pages| pages.to_bytes().unwrap_or_default() as i32,
                    )
            },
        ))
    }

    fn free(ctx: &mut Store<HostState>) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |_caller: Caller<'_, HostState>, _: i32| 0,
        ))
    }

    fn gr_read(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, at: u32, len: i32, buff: i32, err: i32| {
                let (at, len, buff, err) = (at as _, len as usize, buff as _, err as _);

                let msg = &caller.data().msg;
                let payload = if at + len <= msg.len() {
                    msg[at..(at + len)].to_vec()
                } else {
                    return Err(Trap::new(TrapCode::MemoryOutOfBounds.trap_message()));
                };

                let len: u32 = memory
                    .clone()
                    .write(caller.as_context_mut(), buff, &payload)
                    .map_err(|e| log::error!("{:?}", e))
                    .is_err()
                    .into();

                memory
                    .clone()
                    .write(caller.as_context_mut(), err, &len.to_le_bytes())
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::new(TrapCode::MemoryOutOfBounds.trap_message())
                    })?;

                Ok(())
            },
        ))
    }

    fn gr_reply(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, ptr: u32, len: i32, _value: i32, _err: i32| {
                let mut result = vec![0; len as usize];

                memory
                    .read(caller.as_context(), ptr as usize, &mut result)
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::new(TrapCode::MemoryOutOfBounds.trap_message())
                    })?;
                caller.data_mut().msg = result;

                Ok(())
            },
        ))
    }

    fn gr_size(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, size_ptr: u32| {
                let size = caller.data().msg.len() as u32;

                memory
                    .clone()
                    .write(
                        caller.as_context_mut(),
                        size_ptr as usize,
                        &size.to_le_bytes(),
                    )
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::new(TrapCode::MemoryOutOfBounds.trap_message())
                    })?;

                Ok(())
            },
        ))
    }

    fn gr_panic(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |caller: Caller<'_, HostState>, ptr: u32, len: i32| {
                let mut buff = Vec::with_capacity(len as usize);
                memory.read(caller, ptr as usize, &mut buff).map_err(|e| {
                    log::error!("{e:?}");
                    Trap::new(TrapCode::MemoryOutOfBounds.trap_message())
                })?;

                log::error!("Panic: {}", String::from_utf8_lossy(&buff));
                Ok(())
            },
        ))
    }

    fn gr_oom_panic(ctx: impl AsContextMut) -> Extern {
        Extern::Func(Func::wrap(ctx, || {
            log::error!("OOM panic occurred");
            Ok(())
        }))
    }

    fn gr_out_of_gas(ctx: impl AsContextMut) -> Extern {
        Extern::Func(Func::wrap(ctx, || {
            log::error!("Out of gas");
            Ok(())
        }))
    }
}
