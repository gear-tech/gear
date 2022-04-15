// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
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

use core::marker::PhantomData;

use crate::env::StoreData;
use crate::memory::MemoryWrap;
use alloc::format;
use alloc::{string::String, vec};
use gear_backend_common::funcs::*;
use gear_backend_common::{IntoErrorCode, OnSuccessCode};
use gear_backend_common::{EXIT_TRAP_STR, LEAVE_TRAP_STR, WAIT_TRAP_STR};
use gear_core::{
    env::Ext,
    ids::{MessageId, ProgramId},
    memory::Memory,
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use wasmtime::Memory as WasmtimeMemory;
use wasmtime::{AsContextMut, Caller, Func, Store, Trap};

pub struct FuncsHandler<E: Ext + 'static> {
    _panthom: PhantomData<E>,
}

fn get_caller_memory<'a, T: Ext>(
    caller: &'a mut Caller<'_, StoreData<T>>,
    mem: &WasmtimeMemory,
) -> MemoryWrap<'a, T> {
    let store = caller.as_context_mut();
    MemoryWrap { mem: *mem, store }
}

fn write_to_caller_memory<'a, T: Ext>(
    caller: &'a mut Caller<'_, StoreData<T>>,
    mem: &WasmtimeMemory,
    offset: usize,
    buffer: &[u8],
) -> Result<(), String> {
    get_caller_memory(caller, mem)
        .write(offset, buffer)
        .map_err(|e| format!("Cannot write mem: {:?}", e))
}

impl<E: Ext + 'static> FuncsHandler<E> {
    pub fn alloc(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, pages: i32| {
            let ext = caller.data().ext.clone();
            let pages = pages as u32;
            let page = ext
                .with_fallible(|ext| {
                    ext.alloc(pages.into(), &mut get_caller_memory(&mut caller, &mem))
                })
                .map_err(Trap::new)?;
            log::debug!("ALLOC PAGES: {} pages at {:?}", pages, page);
            Ok(page.0)
        };
        Func::wrap(store, func)
    }

    pub fn block_height(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with(|ext: &mut E| ext.block_height()).unwrap_or(0) as i32
        };
        Func::wrap(store, f)
    }

    pub fn block_timestamp(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with(|ext: &mut E| ext.block_timestamp()).unwrap_or(0) as i64
        };
        Func::wrap(store, f)
    }

    pub fn exit_code(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            let reply_tuple = ext.with(|ext: &mut E| ext.reply_to()).map_err(Trap::new)?;
            if let Some((_, exit_code)) = reply_tuple {
                Ok(exit_code)
            } else {
                Err(Trap::new("Not running in the reply context"))
            }
        };
        Func::wrap(store, f)
    }

    pub fn free(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, page: i32| {
            let ext = &caller.data().ext;
            let page = page as u32;
            ext.with(|ext: &mut E| ext.free(page.into()))
                .map_err(Trap::new)?
                .map_err(Trap::new)?;
            if let Err(e) = ext
                .with(|ext: &mut E| ext.free(page.into()))
                .map_err(Trap::new)?
            {
                log::debug!("FREE PAGE ERROR: {:?}", e);
            } else {
                log::debug!("FREE PAGE: {}", page);
            }
            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let f = move |mut caller: Caller<'_, StoreData<E>>, str_ptr: i32, str_len: i32| {
            let ext = caller.data().ext.clone();
            let str_ptr = str_ptr as u32 as usize;
            let str_len = str_len as u32 as usize;
            ext.with_fallible(|ext: &mut E| -> Result<(), &'static str> {
                let mut data = vec![0u8; str_len];
                let mem = get_caller_memory(&mut caller, &mem);
                mem.read(str_ptr, &mut data)
                    .map_err(|_| "Cannot read memory")?;
                match String::from_utf8(data) {
                    Ok(s) => ext.debug(&s),
                    Err(_) => Err("Failed to parse debug string"),
                }
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, f)
    }

    pub fn gas(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, val: i32| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.charge_gas(val as _))
                .map_err(|e| {
                    if gear_backend_common::funcs::is_gas_allowance_trap(e) {
                        Trap::new(e)
                    } else {
                        Trap::new("Trapping: unable to report about gas used")
                    }
                })
        };
        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with(|ext: &mut E| ext.gas_available()).unwrap_or(0) as i64
        };
        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, program_id_ptr: i32| -> Result<(), Trap> {
                let ext = caller.data().ext.clone();
                ext.with(|ext: &mut E| {
                    let value_dest: ProgramId = get_bytes32(
                        &get_caller_memory(&mut caller, &mem),
                        program_id_ptr as u32 as _,
                    )?
                    .into();
                    ext.exit(value_dest)
                })
                .map_err(Trap::new)?
                .map_err(Trap::new)?;

                // Intentionally return an error to break the execution
                Err(Trap::new(EXIT_TRAP_STR))
            };
        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, origin_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let id = ext.origin();
                write_to_caller_memory(&mut caller, &mem, origin_ptr as _, id.as_ref())
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn msg_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, msg_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let message_id = ext.message_id();
                write_to_caller_memory(
                    &mut caller,
                    &mem,
                    msg_id_ptr as isize as _,
                    message_id.as_ref(),
                )
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, at: i32, len: i32, dest: i32| {
            let ext = caller.data().ext.clone();
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            ext.with(|ext: &mut E| {
                let msg = ext.msg().to_vec();
                write_to_caller_memory(&mut caller, &mem, dest as _, &msg[at..(at + len)])
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<i32, String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                ext.reply(ReplyPacket::new(payload, value))
                    .on_success_code(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send reply message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_commit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, message_id_ptr: i32, value_ptr: i32| {
                let ext = caller.data().ext.clone();
                ext.with(|ext: &mut E| -> Result<i32, String> {
                    let mem_wrap = get_caller_memory(&mut caller, &mem);
                    let value = get_u128(&mem_wrap, value_ptr as usize)?;
                    ext.reply_commit(ReplyPacket::new(Default::default(), value))
                        .on_success_code(|message_id| {
                            write_to_caller_memory(
                                &mut caller,
                                &mem,
                                message_id_ptr as isize as _,
                                message_id.as_ref(),
                            )
                        })
                })
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to send message")
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn reply_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, payload_ptr: i32, payload_len: i32| {
                let ext = caller.data().ext.clone();
                ext.with(|ext: &mut E| {
                    let mem_wrap = get_caller_memory(&mut caller, &mem);
                    let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                    ext.reply_push(&payload).into_error_code()
                })
                .map_err(Trap::new)?
                .map_err(|_| "Trapping: unable to push payload into reply")
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, dest: i32| {
            let ext = &caller.data().ext;
            let message_id = match ext.with(|ext: &mut E| ext.reply_to()).map_err(Trap::new)? {
                Some((m_id, _)) => m_id,
                None => return Err(Trap::new("Not running in the reply context")),
            };
            write_to_caller_memory(&mut caller, &mem, dest as isize as _, message_id.as_ref())
                .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<i32, String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                ext.send(HandlePacket::new(dest, payload, value))
                    .on_success_code(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<i32, String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                ext.send(HandlePacket::new_with_gas(
                    dest,
                    payload,
                    gas_limit as _,
                    value,
                ))
                .on_success_code(|message_id| {
                    write_to_caller_memory(
                        &mut caller,
                        &mem,
                        message_id_ptr as isize as _,
                        message_id.as_ref(),
                    )
                })
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_commit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<i32, String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                ext.send_commit(
                    handle_ptr as _,
                    HandlePacket::new(dest, Default::default(), value),
                )
                .on_success_code(|message_id| {
                    write_to_caller_memory(
                        &mut caller,
                        &mem,
                        message_id_ptr as isize as _,
                        message_id.as_ref(),
                    )
                })
            })
            .map_err(Trap::new)?
            .map_err(|e| format!("Trapping: unable to commit and send message: {}", e))
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_commit_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         gas_limit: i64,
                         value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<i32, String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                ext.send_commit(
                    handle_ptr as _,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit as _, value),
                )
                .on_success_code(|message_id| {
                    write_to_caller_memory(
                        &mut caller,
                        &mem,
                        message_id_ptr as isize as _,
                        message_id.as_ref(),
                    )
                })
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to commit and send message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_init(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, handle_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                ext.send_init().on_success_code(|handle| {
                    write_to_caller_memory(
                        &mut caller,
                        &mem,
                        handle_ptr as _,
                        &handle.to_le_bytes(),
                    )
                })
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to init message")
            .map_err(Trap::new)
            .map(|handle| handle as i32)
        };
        Func::wrap(store, func)
    }

    pub fn send_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                ext.send_push(handle_ptr as _, &payload).into_error_code()
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to push payload into message")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn create_program_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         program_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<(), String> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let code_hash = get_bytes32(&mem_wrap, code_hash_ptr as usize)?;
                let salt = get_vec(&mem_wrap, salt_ptr as usize, salt_len as usize)?;
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let new_actor_id = ext.create_program(InitPacket::new_with_gas(
                    code_hash.into(),
                    salt,
                    payload,
                    gas_limit as _,
                    value,
                ))?;
                write_to_caller_memory(
                    &mut caller,
                    &mem,
                    program_id_ptr as isize as _,
                    new_actor_id.as_ref(),
                )
            })
            .map_err(Trap::new)?
            .map_err(|_| "Trapping: unable to create a new program")
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with(|ext: &mut E| ext.msg().len() as _).unwrap_or(0)
        };
        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, source_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let source = ext.source();
                write_to_caller_memory(&mut caller, &mem, source_ptr as _, source.as_ref())
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn program_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, source_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let actor_id = ext.program_id();
                write_to_caller_memory(&mut caller, &mem, source_ptr as _, actor_id.as_ref())
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<(), String> {
                let mut mem_wrap = get_caller_memory(&mut caller, &mem);
                set_u128(&mut mem_wrap, value_ptr as usize, ext.value())
                    .map_err(|e| format!("Cannot set u128: {:?}", e))
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value_available(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| -> Result<(), String> {
                let mut mem_wrap = get_caller_memory(&mut caller, &mem);
                set_u128(&mut mem_wrap, value_ptr as usize, ext.value_available())
                    .map_err(|e| format!("Cannot set u128: {:?}", e))
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &caller.data().ext;
            let _ = ext.with(|ext: &mut E| ext.leave()).map_err(Trap::new)?;
            // Intentionally return an error to break the execution
            Err(Trap::new(LEAVE_TRAP_STR))
        };
        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &caller.data().ext;
            let _ = ext.with(|ext: &mut E| ext.wait()).map_err(Trap::new)?;
            // Intentionally return an error to break the execution
            Err(Trap::new(WAIT_TRAP_STR))
        };
        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, waker_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with(|ext: &mut E| {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let waker_id: MessageId = get_bytes32(&mem_wrap, waker_id_ptr as usize)?.into();
                ext.wake(waker_id)
            })
            .map_err(Trap::new)?
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }
}
