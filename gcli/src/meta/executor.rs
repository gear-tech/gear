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
    pub msg: Vec<u8>,
}

/// Executes the WASM code.
pub fn execute(wasm: &[u8], method: &str) -> Result<Vec<u8>> {
    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        PAGE_STORAGE_PREFIX
    ));

    let engine = Engine::default();
    let module = Module::new(&engine, wasm).unwrap();

    let mut store = Store::new(&engine, HostState::default());
    let mut linker = <Linker<HostState>>::new();

    // Execution environment
    {
        let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None)).unwrap();
        linker.define("env", "memory", Extern::Memory(memory))?;
        linker.define("env", "gr_read", funcs::gr_read(&mut store, memory))?;
        linker.define("env", "alloc", funcs::alloc(&mut store, memory))?;
        linker.define("env", "free", funcs::free(&mut store))?;
        linker.define("env", "gr_size", funcs::gr_size(&mut store, memory))?;
        linker.define("env", "gr_reply", funcs::gr_reply(&mut store, memory))?;
        linker.define("env", "gr_panic", funcs::gr_panic(&mut store, memory))?;
        linker.define("env", "gr_oom_panic", funcs::gr_oom_panic(&mut store))?;
        linker.define("env", "gr_out_of_gas", funcs::gr_out_of_gas(&mut store))?;
        linker.define("env", "gr_block_height", funcs::gr_block_height(&mut store))?;
        linker.define(
            "env",
            "gr_block_timestamp",
            funcs::gr_block_timestamp(&mut store),
        )?;
    }

    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)?;

    let metadata = instance
        .get_export(&store, method)
        .and_then(Extern::into_func)
        .ok_or_else(|| anyhow!("could not find function \"{}\"", method))?
        .typed::<(), (), _>(&mut store)?;

    metadata.call(&mut store, ())?;
    Ok(store.state().msg.clone())
}

mod funcs {
    use super::HostState;
    use wasmi::{
        core::{memory_units::Pages, Trap, TrapCode},
        AsContext, AsContextMut, Caller, Extern, Func, Memory, Store,
    };

    pub fn alloc(store: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            store,
            move |mut caller: Caller<'_, HostState>, pages: i32| {
                memory
                    .clone()
                    .grow(caller.as_context_mut(), Pages(pages as usize))
                    .map_or_else(
                        |err| {
                            log::error!("{err:?}");
                            u32::MAX as i32
                        },
                        |pages| pages.0 as i32,
                    )
            },
        ))
    }

    pub fn free(ctx: impl AsContextMut) -> Extern {
        Extern::Func(Func::wrap(ctx, |_: i32| 0))
    }

    pub fn gr_panic(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |caller: Caller<'_, HostState>, ptr: u32, len: i32| {
                let mut buff = Vec::with_capacity(len as usize);
                memory.read(caller, ptr as usize, &mut buff).map_err(|e| {
                    log::error!("{e:?}");
                    Trap::Code(TrapCode::MemoryAccessOutOfBounds)
                })?;

                log::error!("Panic: {}", String::from_utf8_lossy(&buff));
                Ok(())
            },
        ))
    }

    pub fn gr_oom_panic(ctx: impl AsContextMut) -> Extern {
        Extern::Func(Func::wrap(ctx, || {
            log::error!("OOM panic occurred");
            Ok(())
        }))
    }

    pub fn gr_read(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, at: u32, len: i32, buff: i32, err: i32| {
                let (at, len, buff, err) = (at as _, len as usize, buff as _, err as _);

                let msg = &caller.host_data().msg;
                let payload = if at + len <= msg.len() {
                    msg[at..(at + len)].to_vec()
                } else {
                    return Err(Trap::Code(TrapCode::MemoryAccessOutOfBounds));
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
                        Trap::Code(TrapCode::MemoryAccessOutOfBounds)
                    })?;

                Ok(())
            },
        ))
    }

    pub fn gr_reply(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, ptr: u32, len: i32, _value: i32, _err: i32| {
                let mut result = vec![0; len as usize];

                memory
                    .read(caller.as_context(), ptr as usize, &mut result)
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::Code(TrapCode::MemoryAccessOutOfBounds)
                    })?;
                caller.host_data_mut().msg = result;

                Ok(())
            },
        ))
    }

    pub fn gr_size(ctx: &mut Store<HostState>, memory: Memory) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |mut caller: Caller<'_, HostState>, size_ptr: u32| {
                let size = caller.host_data().msg.len() as u32;

                memory
                    .clone()
                    .write(
                        caller.as_context_mut(),
                        size_ptr as usize,
                        &size.to_le_bytes(),
                    )
                    .map_err(|e| {
                        log::error!("{:?}", e);
                        Trap::Code(TrapCode::MemoryAccessOutOfBounds)
                    })?;

                Ok(())
            },
        ))
    }

    pub fn gr_out_of_gas(ctx: &mut Store<HostState>) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |_caller: Caller<'_, HostState>| Ok(()),
        ))
    }

    pub fn gr_block_height(ctx: &mut Store<HostState>) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |_caller: Caller<'_, HostState>, _height: u32| Ok(()),
        ))
    }

    pub fn gr_block_timestamp(ctx: &mut Store<HostState>) -> Extern {
        Extern::Func(Func::wrap(
            ctx,
            move |_caller: Caller<'_, HostState>, _timestamp: u32| Ok(()),
        ))
    }
}
