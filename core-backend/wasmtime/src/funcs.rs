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

use crate::{env::StoreData, memory::MemoryWrap};
use alloc::{
    string::{FromUtf8Error, String, ToString},
    vec,
};
use codec::Encode;
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    funcs::*,
    AsTerminationReason, IntoExtInfo, TerminationReason, TerminationReasonKind, TrapExplanation,
};
use gear_core::{
    env::{Ext, ExtCarrierWithError},
    ids::{MessageId, ProgramId},
    memory::Memory,
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError};
use wasmtime::{AsContextMut, Caller, Func, Memory as WasmtimeMemory, Store, Trap};

pub struct FuncsHandler<E: Ext + 'static> {
    _panthom: PhantomData<E>,
}

#[derive(Debug, derive_more::Display)]
enum FuncError<E> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    SetU128(MemoryError),
    #[display(fmt = "{}", _0)]
    LaterExtWith(ExtCarrierWithError),
    #[display(fmt = "Failed to parse debug string: {}", _0)]
    DebugString(FromUtf8Error),
    #[display(fmt = "Not running in the reply context")]
    NoReplyContext,
    #[display(fmt = "`gr_exit` has been called")]
    Exit,
    #[display(fmt = "`gr_leave` has been called")]
    Leave,
    #[display(fmt = "`gr_wait` has been called")]
    Wait,
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Unable to call a forbidden function")]
    ForbiddenFunction,
}

impl<E> FuncError<E> {
    fn as_core(&self) -> Option<&E> {
        match self {
            Self::Core(err) => Some(err),
            _ => None,
        }
    }
}

impl<E> From<ExtCarrierWithError> for FuncError<E> {
    fn from(err: ExtCarrierWithError) -> Self {
        Self::LaterExtWith(err)
    }
}

impl<E> From<MemoryError> for FuncError<E> {
    fn from(err: MemoryError) -> Self {
        Self::Memory(err)
    }
}

// for Trap::new
impl<E> From<FuncError<E>> for String
where
    E: CoreError,
{
    fn from(err: FuncError<E>) -> Self {
        err.to_string()
    }
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
) -> Result<(), FuncError<T::Error>> {
    get_caller_memory(caller, mem)
        .write(offset, buffer)
        .map_err(FuncError::Memory)
}

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn alloc(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, pages: i32| {
            let ext = caller.data().ext.clone();
            let pages = pages as u32;
            let page = ext
                .with_fallible(|ext| {
                    ext.alloc(pages.into(), &mut get_caller_memory(&mut caller, &mem))
                        .map_err(FuncError::Core)
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
            ext.with_fallible(|ext| ext.block_height().map_err(FuncError::Core))
                .unwrap_or(0) as i32
        };
        Func::wrap(store, f)
    }

    pub fn block_timestamp(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.block_timestamp().map_err(FuncError::Core))
                .unwrap_or(0) as i64
        };
        Func::wrap(store, f)
    }

    pub fn exit_code(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.reply_to().map_err(FuncError::Core))
                .and_then(|v| v.ok_or(FuncError::NoReplyContext))
                .map(|(_, exit_code)| exit_code)
                .map_err(Trap::new)
        };
        Func::wrap(store, f)
    }

    pub fn free(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, page: i32| {
            let ext = &caller.data().ext;
            let page = page as u32;
            if let Err(err) =
                ext.with_fallible(|ext| ext.free(page.into()).map_err(FuncError::Core))
            {
                log::error!("FREE PAGE ERROR: {}", err);
                Err(Trap::new(err))
            } else {
                log::debug!("FREE PAGE: {}", page);
                Ok(())
            }
        };
        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let f = move |mut caller: Caller<'_, StoreData<E>>, str_ptr: i32, str_len: i32| {
            let ext = caller.data().ext.clone();
            let str_ptr = str_ptr as u32 as usize;
            let str_len = str_len as u32 as usize;
            ext.with_fallible(|ext| -> Result<(), FuncError<_>> {
                let mut data = vec![0u8; str_len];
                let mem = get_caller_memory(&mut caller, &mem);
                mem.read(str_ptr, &mut data)?;
                let s = String::from_utf8(data).map_err(FuncError::DebugString)?;
                ext.debug(&s).map_err(FuncError::Core)?;
                Ok(())
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, f)
    }

    pub fn gas(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, val: i32| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.gas(val as _).map_err(FuncError::Core))
                .map_err(|e| {
                    if let Some(TerminationReasonKind::GasAllowanceExceeded) = e
                        .as_core()
                        .and_then(AsTerminationReason::as_termination_reason)
                    {
                        caller.data_mut().termination_reason =
                            Some(TerminationReason::GasAllowanceExceeded);
                    }

                    Trap::new(e)
                })
        };
        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.gas_available().map_err(FuncError::Core))
                .unwrap_or(0) as i64
        };
        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, program_id_ptr: i32| -> Result<(), Trap> {
                let ext = caller.data().ext.clone();

                if let Err(err) = ext.with_fallible(|ext| -> Result<_, FuncError<_>> {
                    let value_dest: ProgramId = get_bytes32(
                        &get_caller_memory(&mut caller, &mem),
                        program_id_ptr as u32 as _,
                    )?
                    .into();
                    ext.exit(value_dest).map_err(FuncError::Core)
                }) {
                    Err(Trap::new(err))
                } else {
                    // Intentionally return an error to break the execution
                    Err(Trap::new(FuncError::<E::Error>::Exit))
                }
            };
        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, origin_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let id = ext.origin().map_err(FuncError::Core)?;
                write_to_caller_memory(&mut caller, &mem, origin_ptr as _, id.as_ref())
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn msg_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, msg_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let message_id = ext.message_id().map_err(FuncError::Core)?;
                write_to_caller_memory(
                    &mut caller,
                    &mem,
                    msg_id_ptr as isize as _,
                    message_id.as_ref(),
                )
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, at: i32, len: i32, dest: i32| {
            let ext = caller.data().ext.clone();
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let msg = ext.msg().to_vec();
                write_to_caller_memory(&mut caller, &mem, dest as _, &msg[at..(at + len)])
            })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;

                let error_len = ext
                    .reply(ReplyPacket::new(payload, value))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;

                let error_len = ext
                    .reply(ReplyPacket::new_with_gas(payload, gas_limit as _, value))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_commit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, value_ptr: i32, message_id_ptr: i32| {
                let ext = caller.data().ext.clone();
                ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                    let mem_wrap = get_caller_memory(&mut caller, &mem);
                    let value = get_u128(&mem_wrap, value_ptr as usize)?;
                    let error_len = ext
                        .reply_commit(ReplyPacket::new(Default::default(), value))
                        .process_error()
                        .map_err(FuncError::Core)?
                        .error_len_on_success(|message_id| {
                            write_to_caller_memory(
                                &mut caller,
                                &mem,
                                message_id_ptr as isize as _,
                                message_id.as_ref(),
                            )
                        })?;
                    Ok(error_len)
                })
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn reply_commit_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .reply_commit(ReplyPacket::new_with_gas(
                        Default::default(),
                        gas_limit as _,
                        value,
                    ))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn reply_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>, payload_ptr: i32, payload_len: i32| {
                let ext = caller.data().ext.clone();
                ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                    let mem_wrap = get_caller_memory(&mut caller, &mem);
                    let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                    let error_len = ext
                        .reply_push(&payload)
                        .process_error()
                        .map_err(FuncError::Core)?
                        .error_len();
                    Ok(error_len)
                })
                .map_err(Trap::new)
            };
        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, dest: i32| {
            let ext = &caller.data().ext;
            ext.with_fallible(|ext| ext.reply_to().map_err(FuncError::Core))
                .and_then(|v| v.ok_or(FuncError::NoReplyContext))
                .and_then(|(msg_id, _)| {
                    write_to_caller_memory(&mut caller, &mem, dest as isize as _, msg_id.as_ref())
                })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .send(HandlePacket::new(dest, payload, value))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;

                let error_len = ext
                    .send(HandlePacket::new_with_gas(
                        dest,
                        payload,
                        gas_limit as _,
                        value,
                    ))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .send_commit(
                        handle_ptr as _,
                        HandlePacket::new(dest, Default::default(), value),
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let dest: ProgramId = get_bytes32(&mem_wrap, program_id_ptr as usize)?.into();
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .send_commit(
                        handle_ptr as _,
                        HandlePacket::new_with_gas(dest, Default::default(), gas_limit as _, value),
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|message_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            message_id_ptr as isize as _,
                            message_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_init(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, handle_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let error_len = ext
                    .send_init()
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|handle| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            handle_ptr as _,
                            &handle.to_le_bytes(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn send_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let error_len = ext
                    .send_push(handle_ptr as _, &payload)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn create_program(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>,
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         program_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext: &mut E| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let code_hash = get_bytes32(&mem_wrap, code_hash_ptr as usize)?;
                let salt = get_vec(&mem_wrap, salt_ptr as usize, salt_len as usize)?;
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .create_program(InitPacket::new(code_hash.into(), salt, payload, value))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|new_actor_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            program_id_ptr as isize as _,
                            new_actor_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
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
            ext.with_fallible(|ext| -> Result<u32, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let code_hash = get_bytes32(&mem_wrap, code_hash_ptr as usize)?;
                let salt = get_vec(&mem_wrap, salt_ptr as usize, salt_len as usize)?;
                let payload = get_vec(&mem_wrap, payload_ptr as usize, payload_len as usize)?;
                let value = get_u128(&mem_wrap, value_ptr as usize)?;
                let error_len = ext
                    .create_program(InitPacket::new_with_gas(
                        code_hash.into(),
                        salt,
                        payload,
                        gas_limit as _,
                        value,
                    ))
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len_on_success(|new_actor_id| {
                        write_to_caller_memory(
                            &mut caller,
                            &mem,
                            program_id_ptr as isize as _,
                            new_actor_id.as_ref(),
                        )
                    })?;
                Ok(error_len)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>| {
            let ext = &caller.data().ext;
            ext.with(|ext| ext.msg().len() as _).unwrap_or(0)
        };
        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, source_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let source = ext.source().map_err(FuncError::Core)?;
                write_to_caller_memory(&mut caller, &mem, source_ptr as _, source.as_ref())
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn program_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, source_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let actor_id = ext.program_id().map_err(FuncError::Core)?;
                write_to_caller_memory(&mut caller, &mem, source_ptr as _, actor_id.as_ref())
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<(), FuncError<E::Error>> {
                let mut mem_wrap = get_caller_memory(&mut caller, &mem);
                let value = ext.value().map_err(FuncError::Core)?;
                set_u128(&mut mem_wrap, value_ptr as usize, value).map_err(FuncError::SetU128)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn value_available(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, value_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<(), FuncError<E::Error>> {
                let mut mem_wrap = get_caller_memory(&mut caller, &mem);
                let value_available = ext.value_available().map_err(FuncError::Core)?;
                set_u128(&mut mem_wrap, value_ptr as usize, value_available)
                    .map_err(FuncError::SetU128)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &caller.data().ext;
            let trap =
                if let Err(err) = ext.with_fallible(|ext| ext.leave().map_err(FuncError::Core)) {
                    Trap::new(err)
                } else {
                    caller.data_mut().termination_reason = Some(TerminationReason::Leave);
                    Trap::new(FuncError::<E::Error>::Leave)
                };
            // Intentionally return an error to break the execution
            Err(trap)
        };
        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &caller.data().ext;
            let trap =
                if let Err(err) = ext.with_fallible(|ext| ext.wait().map_err(FuncError::Core)) {
                    Trap::new(err)
                } else {
                    caller.data_mut().termination_reason = Some(TerminationReason::Wait);
                    Trap::new(FuncError::<E::Error>::Wait)
                };
            // Intentionally return an error to break the execution
            Err(trap)
        };
        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, waker_id_ptr: i32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<_, FuncError<E::Error>> {
                let mem_wrap = get_caller_memory(&mut caller, &mem);
                let waker_id: MessageId = get_bytes32(&mem_wrap, waker_id_ptr as usize)?.into();
                ext.wake(waker_id).map_err(FuncError::Core)
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, data_ptr: u32| {
            let ext = caller.data().ext.clone();
            ext.with_fallible(|ext| -> Result<(), FuncError<E::Error>> {
                let mut mem_wrap = get_caller_memory(&mut caller, &mem);
                let err = ext.last_error().ok_or(FuncError::SyscallErrorExpected)?;
                let err = err.encode();
                mem_wrap.write(data_ptr as usize, &err)?;
                Ok(())
            })
            .map_err(Trap::new)
        };
        Func::wrap(store, func)
    }

    pub fn forbidden(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            caller.data_mut().termination_reason = Some(TerminationReason::Trap {
                explanation: Some(TrapExplanation::Other(
                    "Unable to call a forbidden function".into(),
                )),
                description: None,
            });
            Err(Trap::new(FuncError::<E::Error>::ForbiddenFunction))
        };
        Func::wrap(store, func)
    }
}
