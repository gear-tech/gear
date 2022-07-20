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

use crate::{context::Context, env::StoreData};
use alloc::{
    string::{FromUtf8Error, String, ToString},
    vec,
};
use codec::Encode;
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    AsTerminationReason, IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{Ext, FunctionContext},
    ids::{MessageId, ProgramId},
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError};
use wasmtime::{AsContextMut, Caller, Func, Memory as WasmtimeMemory, Store, Trap};

pub struct FuncsHandler<E: Ext + 'static>(PhantomData<E>);

#[derive(Debug, derive_more::Display)]
pub enum FuncError<E> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    SetU128(MemoryError),
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

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn alloc(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, pages: i32| {
            let ctx = Context { caller }.rc();
            let pages = pages as u32;
            let page = ctx
                .borrow_mut()
                .data_mut()
                .ext
                .alloc(
                    pages.into(),
                    &mut crate::memory::MemoryWrap {
                        mem,
                        store: ctx.borrow_mut().as_context_mut(),
                    },
                )
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;
            log::debug!("ALLOC PAGES: {} pages at {:?}", pages, page);
            Ok(page.0)
        };
        Func::wrap(store, func)
    }

    pub fn block_height(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |mut caller: Caller<'_, StoreData<E>>| {
            let ext = &mut caller.data_mut().ext;
            ext.block_height()
                .map_err(FuncError::<E::Error>::Core)
                .unwrap_or(0) as i32
        };
        Func::wrap(store, f)
    }

    pub fn block_timestamp(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |mut caller: Caller<'_, StoreData<E>>| {
            let ext = &mut caller.data_mut().ext;
            ext.block_timestamp()
                .map_err(FuncError::<E::Error>::Core)
                .unwrap_or(0) as i64
        };
        Func::wrap(store, f)
    }

    pub fn exit_code(store: &mut Store<StoreData<E>>) -> Func {
        let f = move |mut caller: Caller<'_, StoreData<E>>| {
            let ext = &mut caller.data_mut().ext;
            ext.reply_details()
                .map_err(FuncError::<E::Error>::Core)
                .and_then(|v| v.ok_or(FuncError::<E::Error>::NoReplyContext))
                .map(|details| details.into_exit_code())
                .map_err(Trap::new)
        };
        Func::wrap(store, f)
    }

    pub fn free(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, page: i32| {
            let ext = &mut caller.data_mut().ext;
            let page = page as u32;
            if let Err(err) = ext.free(page.into()).map_err(FuncError::<E::Error>::Core) {
                log::debug!("FREE PAGE ERROR: {}", err);
                Err(Trap::new(err))
            } else {
                log::debug!("FREE PAGE: {}", page);
                Ok(())
            }
        };
        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let f = move |caller: Caller<'_, StoreData<E>>,
                      str_ptr: i32,
                      str_len: i32|
              -> Result<_, Trap> {
            let mut ctx = Context { caller };

            let str_ptr = str_ptr as u32 as usize;
            let str_len = str_len as u32 as usize;

            let mut data = vec![0u8; str_len];
            ctx.read_memory_into(&mem, str_ptr, &mut data)?;

            let s = String::from_utf8(data)
                .map_err(|e| Trap::new(FuncError::<E::Error>::DebugString(e)))?;

            ctx.ext_mut()
                .debug(&s)
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            Ok(())
        };
        Func::wrap(store, f)
    }

    pub fn gas(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>, val: i32| {
            let ext = &mut caller.data_mut().ext;
            ext.gas(val as _)
                .map_err(FuncError::<E::Error>::Core)
                .map_err(|e| {
                    if let Some(TerminationReason::GasAllowanceExceeded) = e
                        .as_core()
                        .and_then(AsTerminationReason::as_termination_reason)
                    {
                        caller.data_mut().termination_reason =
                            TerminationReason::GasAllowanceExceeded;
                    }

                    Trap::new(e)
                })
        };
        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<i64, Trap> {
            let ext = &mut caller.data_mut().ext;

            Ok(ext
                .gas_available()
                .map_err(FuncError::<E::Error>::Core)
                .unwrap_or(0) as i64)
        };
        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         program_id_ptr: i32|
              -> Result<(), Trap> {
            let mut ctx = Context { caller };

            let program_id: ProgramId = ctx.get_bytes32(&mem, program_id_ptr as u32 as _)?.into();

            ctx.ext_mut()
                .exit()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            *ctx.termination_reason() = TerminationReason::Exit(program_id);
            Err(Trap::new(FuncError::<E::Error>::Exit))
        };
        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, origin_ptr: i32| {
            let mut ctx = Context { caller };
            let id = ctx
                .ext_mut()
                .origin()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;
            ctx.write_into_memory(&mem, origin_ptr as _, id.as_ref())
        };
        Func::wrap(store, func)
    }

    pub fn msg_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, msg_id_ptr: i32| -> Result<_, Trap> {
            let mut ctx = Context { caller };
            let message_id = ctx
                .ext_mut()
                .message_id()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            ctx.write_into_memory(&mem, msg_id_ptr as isize as _, message_id.as_ref())
        };
        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, at: i32, len: i32, dest: i32| {
            let mut ctx = Context { caller };
            let at = at as u32 as usize;
            let len = len as u32 as usize;
            let msg = ctx.ext_mut().msg().to_vec();

            ctx.write_into_memory(&mem, dest as _, &msg[at..(at + len)])
        };
        Func::wrap(store, func)
    }

    pub fn reply(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;

            let error_len = ctx
                .ext_mut()
                .reply(ReplyPacket::new(payload, value))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn reply_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32| {
            let mut ctx = Context { caller };
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;

            let error_len = ctx
                .ext_mut()
                .reply(ReplyPacket::new_with_gas(payload, gas_limit as _, value))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn reply_commit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         value_ptr: i32,
                         message_id_ptr: i32|
              -> Result<_, Trap> {
            let mut ctx = Context { caller };
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .reply_commit(ReplyPacket::new(Default::default(), value))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn reply_commit_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32|
              -> Result<_, Trap> {
            let mut ctx = Context { caller };
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .reply_commit(ReplyPacket::new_with_gas(
                    Default::default(),
                    gas_limit as _,
                    value,
                ))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn reply_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         payload_ptr: i32,
                         payload_len: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let error_len = ctx
                .ext_mut()
                .reply_push(&payload)
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len();

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, dest: i32| {
            let mut ctx = Context { caller };

            ctx.ext_mut()
                .reply_details()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))
                .and_then(|v| v.ok_or_else(|| Trap::new(FuncError::<E::Error>::NoReplyContext)))
                .and_then(|details| {
                    ctx.write_into_memory(
                        &mem,
                        dest as isize as _,
                        details.into_reply_to().as_ref(),
                    )
                })?;

            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn send(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };

            let dest: ProgramId = ctx.get_bytes32(&mem, program_id_ptr as usize)?.into();
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .send(HandlePacket::new(dest, payload, value))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn send_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         message_id_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let dest: ProgramId = ctx.get_bytes32(&mem, program_id_ptr as usize)?.into();
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;

            let error_len = ctx
                .ext_mut()
                .send(HandlePacket::new_with_gas(
                    dest,
                    payload,
                    gas_limit as _,
                    value,
                ))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn send_commit(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         value_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let dest: ProgramId = ctx.get_bytes32(&mem, program_id_ptr as usize)?.into();
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .send_commit(
                    handle_ptr as _,
                    HandlePacket::new(dest, Default::default(), value),
                )
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn send_commit_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         gas_limit: i64,
                         value_ptr: i32|
              -> Result<_, Trap> {
            let mut ctx = Context { caller };
            let dest: ProgramId = ctx.get_bytes32(&mem, program_id_ptr as usize)?.into();
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .send_commit(
                    handle_ptr as _,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit as _, value),
                )
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|message_id| {
                    ctx.write_into_memory(&mem, message_id_ptr as isize as _, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn send_init(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, handle_ptr: i32| -> Result<_, Trap> {
            let mut ctx = Context { caller };
            let error_len = ctx
                .ext_mut()
                .send_init()
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|handle| {
                    ctx.write_into_memory(&mem, handle_ptr as _, &handle.to_le_bytes())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn send_push(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         handle_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32| {
            let mut ctx = Context { caller };
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let error_len = ctx
                .ext_mut()
                .send_push(handle_ptr as _, &payload)
                .process_error()
                .map_err(|e| Trap::new(FuncError::Core(e)))?
                .error_len();

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn create_program(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         program_id_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let code_hash = ctx.get_bytes32(&mem, code_hash_ptr as usize)?;
            let salt = ctx.get_vec(&mem, salt_ptr as usize, salt_len as usize)?;
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .create_program(InitPacket::new(code_hash.into(), salt, payload, value))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|new_actor_id| {
                    ctx.write_into_memory(&mem, program_id_ptr as isize as _, new_actor_id.as_ref())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn create_program_wgas(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>,
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: i64,
                         value_ptr: i32,
                         program_id_ptr: i32|
              -> Result<u32, Trap> {
            let mut ctx = Context { caller };
            let code_hash = ctx.get_bytes32(&mem, code_hash_ptr as usize)?;
            let salt = ctx.get_vec(&mem, salt_ptr as usize, salt_len as usize)?;
            let payload = ctx.get_vec(&mem, payload_ptr as usize, payload_len as usize)?;
            let value = ctx.get_u128(&mem, value_ptr as usize)?;
            let error_len = ctx
                .ext_mut()
                .create_program(InitPacket::new_with_gas(
                    code_hash.into(),
                    salt,
                    payload,
                    gas_limit as _,
                    value,
                ))
                .process_error()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?
                .error_len_on_success(|new_actor_id| {
                    ctx.write_into_memory(&mem, program_id_ptr as isize as _, new_actor_id.as_ref())
                })?;

            Ok(error_len)
        };
        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<StoreData<E>>) -> Func {
        let func =
            move |mut caller: Caller<'_, StoreData<E>>| caller.data_mut().ext.msg().len() as i64;
        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, source_ptr: i32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let source = ctx
                .ext_mut()
                .source()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;
            ctx.write_into_memory(&mem, source_ptr as _, source.as_ref())?;

            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn program_id(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, source_ptr: i32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let actor_id = ctx
                .ext_mut()
                .program_id()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            ctx.write_into_memory(&mem, source_ptr as _, actor_id.as_ref())?;
            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, value_ptr: i32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let value_available = ctx
                .ext_mut()
                .value()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            ctx.set_u128(&mem, value_ptr as usize, value_available)?;

            Ok(())
        };

        Func::wrap(store, func)
    }

    pub fn value_available(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, value_ptr: i32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let value_available = ctx
                .ext_mut()
                .value_available()
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;

            ctx.set_u128(&mem, value_ptr as usize, value_available)?;

            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &mut caller.data_mut().ext;
            let trap = if let Err(err) = ext.leave().map_err(FuncError::<E::Error>::Core) {
                Trap::new(err)
            } else {
                caller.data_mut().termination_reason = TerminationReason::Leave;
                Trap::new(FuncError::<E::Error>::Leave)
            };
            // Intentionally return an error to break the execution
            Err(trap)
        };
        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            let ext = &mut caller.data_mut().ext;

            let trap = if let Err(err) = ext.wait().map_err(FuncError::<E::Error>::Core) {
                Trap::new(err)
            } else {
                caller.data_mut().termination_reason = TerminationReason::Wait;
                Trap::new(FuncError::<E::Error>::Wait)
            };

            // Intentionally return an error to break the execution
            Err(trap)
        };
        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, waker_id_ptr: i32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let waker_id: MessageId = ctx.get_bytes32(&mem, waker_id_ptr as usize)?.into();

            ctx.ext_mut()
                .wake(waker_id)
                .map_err(|e| Trap::new(FuncError::<E::Error>::Core(e)))?;
            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<StoreData<E>>, mem: WasmtimeMemory) -> Func {
        let func = move |caller: Caller<'_, StoreData<E>>, data_ptr: u32| -> Result<(), Trap> {
            let mut ctx = Context { caller };
            let err = ctx
                .ext_mut()
                .last_error()
                .ok_or_else(|| Trap::new(FuncError::<E::Error>::SyscallErrorExpected))?;
            let err = err.encode();
            ctx.write_into_memory(&mem, data_ptr as usize, &err)?;

            Ok(())
        };
        Func::wrap(store, func)
    }

    pub fn forbidden(store: &mut Store<StoreData<E>>) -> Func {
        let func = move |mut caller: Caller<'_, StoreData<E>>| -> Result<(), Trap> {
            caller.data_mut().termination_reason =
                TerminationReason::Trap(TrapExplanation::ForbiddenFunction);
            Err(Trap::new(FuncError::<E::Error>::ForbiddenFunction))
        };
        Func::wrap(store, func)
    }
}
