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

use crate::env::Runtime;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{
    string::{FromUtf8Error, String},
    vec,
    vec::Vec,
};
use codec::Encode;
use core::{
    convert::{TryFrom, TryInto},
    fmt,
    marker::PhantomData,
    slice::Iter,
};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    funcs, AsTerminationReason, IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::{Ext, ExtCarrierWithError},
    ids::{MessageId, ProgramId},
    memory::Memory,
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gear_core_errors::MemoryError;
use sp_sandbox::{HostError, ReturnValue, SandboxMemory, Value};

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

pub(crate) fn pop_i32<T: TryFrom<i32>>(arg: &mut Iter<'_, Value>) -> Result<T, HostError> {
    match arg.next() {
        Some(Value::I32(val)) => Ok((*val).try_into().map_err(|_| HostError)?),
        _ => Err(HostError),
    }
}

pub(crate) fn pop_i64<T: TryFrom<i64>>(arg: &mut Iter<'_, Value>) -> Result<T, HostError> {
    match arg.next() {
        Some(Value::I64(val)) => Ok((*val).try_into().map_err(|_| HostError)?),
        _ => Err(HostError),
    }
}

pub(crate) fn return_i32<T: TryInto<i32>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| Value::I32(v).into())
        .map_err(|_| HostError)
}

pub(crate) fn return_i64<T: TryInto<i64>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| Value::I64(v).into())
        .map_err(|_| HostError)
}

pub fn get_bytes32(
    mem: &sp_sandbox::default_executor::Memory,
    ptr: usize,
) -> Result<[u8; 32], MemoryError> {
    let mut ret = [0u8; 32];
    mem.get(ptr as u32, &mut ret)
        .map_err(|_| MemoryError::MemoryAccessError)?;
    Ok(ret)
}

pub fn get_u128(
    mem: &sp_sandbox::default_executor::Memory,
    ptr: usize,
) -> Result<u128, MemoryError> {
    let mut u128_le = [0u8; 16];
    mem.get(ptr as u32, &mut u128_le)
        .map_err(|_| MemoryError::MemoryAccessError)?;
    Ok(u128::from_le_bytes(u128_le))
}

pub fn get_vec(
    mem: &sp_sandbox::default_executor::Memory,
    ptr: usize,
    len: usize,
) -> Result<Vec<u8>, MemoryError> {
    let mut vec = vec![0u8; len];
    mem.get(ptr as u32, &mut vec)
        .map_err(|_| MemoryError::MemoryAccessError)?;
    Ok(vec.to_vec())
}

pub fn set_u128(
    mem: &sp_sandbox::default_executor::Memory,
    ptr: usize,
    val: u128,
) -> Result<(), MemoryError> {
    mem.set(ptr as u32, &val.to_le_bytes())
        .map_err(|_| MemoryError::MemoryAccessError)
}

fn wto<E>(
    memory: &sp_sandbox::default_executor::Memory,
    ptr: usize,
    buff: &[u8],
) -> Result<(), FuncError<E>> {
    memory
        .set(ptr as u32, buff)
        .map_err(|_| MemoryError::MemoryAccessError)
        .map_err(FuncError::Memory)
}

#[derive(Debug, derive_more::Display)]
pub enum FuncError<E> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[display(fmt = "{}", _0)]
    LaterExtWith(ExtCarrierWithError),
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "Cannot set u128: {}", _0)]
    SetU128(MemoryError),
    #[display(fmt = "Exit code ran into non-reply scenario")]
    NonReplyExitCode,
    #[display(fmt = "Not running in reply context")]
    NoReplyContext,
    #[display(fmt = "Failed to parse debug string: {}", _0)]
    DebugString(FromUtf8Error),
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {:?}", _0)]
    Terminated(TerminationReason),
}

impl<E> FuncError<E>
where
    E: fmt::Display,
{
    fn as_core(&self) -> Option<&E> {
        match self {
            Self::Core(err) => Some(err),
            _ => None,
        }
    }

    pub fn into_termination_reason(self) -> TerminationReason {
        match self {
            Self::Terminated(reason) => reason,
            err => TerminationReason::Trap(TrapExplanation::Other(err.to_string().into())),
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

pub(crate) struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let dest: ProgramId = get_bytes32(&ctx.memory, program_id_ptr)?.into();
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .send(HandlePacket::new(dest, payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };

        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let dest: ProgramId = get_bytes32(&ctx.memory, program_id_ptr)?.into();
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;

            let error_len = ctx.ext
                .send(HandlePacket::new_with_gas(dest, payload, gas_limit, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let dest: ProgramId = get_bytes32(&ctx.memory, program_id_ptr)?.into();
            let value = get_u128(&ctx.memory, value_ptr)?;

            let error_len = ctx.ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new(dest, Default::default(), value),
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn send_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;
        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let dest: ProgramId = get_bytes32(&ctx.memory, program_id_ptr)?.into();
            let value = get_u128(&ctx.memory, value_ptr)?;

            let error_len = ctx.ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit, value),
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let error_len = ctx.ext
                .send_init()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|handle| wto(&ctx.memory, handle_ptr, &handle.to_le_bytes()))?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let error_len = ctx.ext
                .send_push(handle_ptr, &payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let at = pop_i32(&mut args)?;
        let len: usize = pop_i32(&mut args)?;
        let dest = pop_i32(&mut args)?;

        // // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let msg = ctx.ext.msg().to_vec();
            wto(&ctx.memory, dest, &msg[at..(at + len)])
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn size(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        return_i32(ctx.ext.msg().len())
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let value_dest_ptr = pop_i32(&mut args.iter())?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut res = || -> Result<(), _> {
            let value_dest: ProgramId = get_bytes32(&ctx.memory, value_dest_ptr)?.into();
            ctx.ext.exit().map_err(FuncError::Core)?;
            Err(FuncError::Terminated(TerminationReason::Exit(value_dest)))
        };
        if let Err(err) = res() {
            ctx.err = err;
        }

        Err(HostError)
    }

    pub fn exit_code(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let reply_tuple = ctx.ext.reply_to().map_err(FuncError::Core).map_err(|e| {
            ctx.err = e;
            HostError
        })?;

        if let Some((_, exit_code)) = reply_tuple {
            return_i32(exit_code)
        } else {
            ctx.err = FuncError::NonReplyExitCode;
            Err(HostError)
        }
    }

    pub fn gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let val = pop_i32(&mut args)?;

        ctx.ext
            .gas(val)
            .map_err(FuncError::Core)
            .map(|()| ReturnValue::Unit)
            .map_err(|e| {
                if let Some(TerminationReason::GasAllowanceExceeded) = e
                    .as_core()
                    .and_then(AsTerminationReason::as_termination_reason)
                    .cloned()
                {
                    ctx.err = FuncError::Terminated(TerminationReason::GasAllowanceExceeded);
                }
                HostError
            })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let pages: u32 = pop_i32(&mut args)?;

        // let Runtime {
        //     ext, memory_wrap, ..
        // } = ctx;

        ctx.ext.alloc(pages.into(), ctx.memory_wrap)
            .map_err(FuncError::Core)
            .map(|page| {
                log::debug!("ALLOC: {} pages at {:?}", pages, page);
                Value::I32(page.0 as i32).into()
            })
            .map_err(|e| {
                ctx.err = e;
                HostError
            })
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let page: u32 = pop_i32(&mut args)?;

        if let Err(err) = ctx.ext.free(page.into()).map_err(FuncError::Core) {
            log::debug!("FREE ERROR: {}", err);
            ctx.err = err;
            Err(HostError)
        } else {
            log::debug!("FREE: {}", page);
            Ok(ReturnValue::Unit)
        }
    }

    pub fn block_height(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_height = ctx
            .ext
            .block_height()
            .map_err(FuncError::Core)
            .map_err(|err| {
                ctx.err = err;
                HostError
            })?;

        return_i32(block_height)
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_timestamp =
            ctx.ext
                .block_timestamp()
                .map_err(FuncError::Core)
                .map_err(|err| {
                    ctx.err = err;
                    HostError
                })?;

        return_i64(block_timestamp)
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let origin_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let origin = ctx.ext.origin().map_err(FuncError::Core)?;
            wto(&ctx.memory, origin_ptr, origin.as_ref())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .reply(ReplyPacket::new(payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .reply_commit(ReplyPacket::new(Default::default(), value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .reply_commit(ReplyPacket::new_with_gas(
                    Default::default(),
                    gas_limit,
                    value,
                ))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    wto(&ctx.memory, message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let dest = pop_i32(&mut args)?;

        let maybe_message_id = ctx.ext.reply_to().map_err(FuncError::Core).map_err(|err| {
            ctx.err = err;
            HostError
        })?;

        if let Some((message_id, _)) = maybe_message_id {
            wto(&ctx.memory, dest, message_id.as_ref()).map_err(|err| {
                ctx.err = err;
                HostError
            })?;

            Ok(ReturnValue::Unit)
        } else {
            ctx.err = FuncError::NoReplyContext;
            Err(HostError)
        }
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let error_len = ctx.ext
                .reply_push(&payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let str_ptr = pop_i32(&mut args)?;
        let str_len = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let mut data = vec![0u8; str_len];
            ctx.memory
                .get(str_ptr, data.as_mut_slice())
                .map_err(|_| MemoryError::MemoryAccessError)?;
            let s = String::from_utf8(data).map_err(FuncError::DebugString)?;
            ctx.ext.debug(&s).map_err(FuncError::Core)?;
            Ok(())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let gas_available = ctx
            .ext
            .gas_available()
            .map_err(FuncError::Core)
            .map_err(|_| HostError)?;

        return_i64(gas_available)
    }

    pub fn msg_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let msg_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let message_id = ctx.ext.message_id().map_err(FuncError::Core)?;
            wto(&ctx.memory, msg_id_ptr, message_id.as_ref())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;

        // // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let program_id = ctx.ext.program_id().map_err(FuncError::Core)?;
            wto(&ctx.memory, program_id_ptr, program_id.as_ref())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let source_ptr = pop_i32(&mut args)?;

        // // let Runtime { ext, memory, .. } = ctx;

        let res = match ctx.ext.source() {
            Ok(source) => wto(&ctx.memory, source_ptr, source.as_ref())
                .map(|()| ReturnValue::Unit)
                .map_err(|err| {
                    ctx.err = err;
                    HostError
                }),
            Err(err) => {
                ctx.err = FuncError::Core(err);
                Err(HostError)
            }
        };
        res
    }

    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let value = ctx.ext.value().map_err(FuncError::Core)?;
            set_u128(&ctx.memory, value_ptr, value).map_err(FuncError::SetU128)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let value_available = ctx.ext.value_available().map_err(FuncError::Core)?;
            set_u128(&ctx.memory, value_ptr, value_available).map_err(FuncError::SetU128)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.err = ctx
            .ext
            .leave()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Leave));
        Err(HostError)
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.err = ctx
            .ext
            .wait()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Wait));
        Err(HostError)
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let waker_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let waker_id: MessageId = get_bytes32(&ctx.memory, waker_id_ptr)?.into();
            ctx.ext.wake(waker_id).map_err(FuncError::Core)
        };
        f().map(|_| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let code_hash_ptr = pop_i32(&mut args)?;
        let salt_ptr = pop_i32(&mut args)?;
        let salt_len = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let code_hash = get_bytes32(&ctx.memory, code_hash_ptr)?;
            let salt = get_vec(&ctx.memory, salt_ptr, salt_len)?;
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .create_program(InitPacket::new(code_hash.into(), salt, payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|new_actor_id| {
                    wto(&ctx.memory, program_id_ptr, new_actor_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn create_program_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let code_hash_ptr = pop_i32(&mut args)?;
        let salt_ptr = pop_i32(&mut args)?;
        let salt_len = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let code_hash = get_bytes32(&ctx.memory, code_hash_ptr)?;
            let salt = get_vec(&ctx.memory, salt_ptr, salt_len)?;
            let payload = get_vec(&ctx.memory, payload_ptr, payload_len)?;
            let value = get_u128(&ctx.memory, value_ptr)?;
            let error_len = ctx.ext
                .create_program(InitPacket::new_with_gas(
                    code_hash.into(),
                    salt,
                    payload,
                    gas_limit,
                    value,
                ))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|new_actor_id| {
                    wto(&ctx.memory, program_id_ptr, new_actor_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| Value::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                HostError
            })
    }

    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
        let mut args = args.iter();

        let data_ptr = pop_i32(&mut args)?;

        // let Runtime { ext, memory, .. } = ctx;

        let mut f = || {
            let err = ctx.ext.last_error().ok_or(FuncError::SyscallErrorExpected)?;
            let err = err.encode();
            wto(&ctx.memory, data_ptr, &err)?;
            Ok(())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.err =
            FuncError::Terminated(TerminationReason::Trap(TrapExplanation::ForbiddenFunction));
        Err(HostError)
    }
}
