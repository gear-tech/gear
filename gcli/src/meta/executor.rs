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

    macro_rules! func {
        ($store:tt) => {
            func!($store,)
        };
        ($store:tt, $($ty:tt),* ) => {
            Extern::Func(Func::wrap(
                $store,
                move |_caller: Caller<'_, HostState>, $(_: $ty),*| { Ok(()) },
            ))
        };
        (@result $store:tt, $($ty:tt),* ) => {
            Extern::Func(Func::wrap(
                $store,
                move |_caller: Caller<'_, HostState>, $(_: $ty),*| { 0 },
            ))
        };
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
                "gr_oom_panic" => gr_oom_panic(store),
                "gr_read" => gr_read(store, memory),
                "gr_reply" => gr_reply(store, memory),
                "gr_panic" => gr_panic(store, memory),
                "gr_size" => gr_size(store, memory),
                // methods may be used by programs but not required by metadata.
                "free" => func!(@result store, i32),
                "free_range" => func!(@result store, i32, i32),
                "gr_block_height" => func!(store, u32),
                "gr_block_timestamp" => func!(store, u32),
                "gr_create_program_wgas" => func!(store, i32, i32, u32, i32, u32, u64, u32, i32),
                "gr_create_program" => func!(store, i32, i32, u32, i32, u32, u64, i32),
                "gr_debug" => func!(store, i32, u32),
                "gr_exit" => func!(store, i32),
                "gr_gas_available" => func!(store, i32),
                "gr_leave" => func!(store),
                "gr_message_id" => func!(store, i32),
                "gr_system_break" => func!(store, u64),
                "gr_pay_program_rent" => func!(store, i32, i32),
                "gr_program_id" => func!(store, i32),
                "gr_random" => func!(store, i32, i32),
                "gr_reply_code" => func!(store, i32),
                "gr_reply_commit" => func!(store, i32, i32),
                "gr_reply_deposit" => func!(store, i32, u64, i32),
                "gr_reply_input" => func!(store, u32, u32, i32, i32),
                "gr_reply_push" => func!(store, i32, u32, i32),
                "gr_reply_push_input" => func!(store, u32, u32, i32),
                "gr_reply_push_input_wgas" => func!(store, u32, u32, u64, i32, i32),
                "gr_reply_to" => func!(store, i32),
                "gr_reply_wgas" => func!(store, i32, u32, u64, i32, i32),
                "gr_reservation_reply" => func!(store, i32, i32, u32, i32),
                "gr_reservation_send_commit" => func!(store, u32, i32, u32, i32),
                "gr_reservation_send" => func!(store, i32, i32, u32, u32, i32),
                "gr_reserve_gas" => func!(store, u64, u32, i32),
                "gr_send" => func!(store, i32, i32, u32, u32, i32),
                "gr_send_commit" => func!(store, u32, i32, u32, i32),
                "gr_send_commit_wgas" => func!(store, u32, i32, u64, u32, i32),
                "gr_send_init" => func!(store, i32),
                "gr_send_input" => func!(store, i32, u32, u32, u32, i32),
                "gr_send_input_wgas" => func!(store, i32, u32, u32, u64, u32, i32),
                "gr_send_push" => func!(store, u32, i32, u32, i32),
                "gr_send_push_input" => func!(store, u32, u32, u32, i32),
                "gr_send_wgas" => func!(store, i32, i32, u32, u64, u32, i32),
                "gr_signal_code" => func!(store, i32),
                "gr_signal_from" => func!(store, i32),
                "gr_source" => func!(store, i32),
                "gr_system_reserve_gas" => func!(store, u64, i32),
                "gr_unreserve_gas" => func!(store, i32, i32),
                "gr_value" => func!(store, i32),
                "gr_wait" => func!(store, u32),
                "gr_wait_for" => func!(store, u32),
                "gr_wait_up_to" => func!(store, u32),
                "gr_wake" => func!(store, i32, u32, i32),
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
}
