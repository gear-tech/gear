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

use crate::runtime::Runtime;
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
    ops::Range,
    slice::Iter,
};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    AsTerminationReason, IntoExtInfo, RuntimeCtx, TerminationReason, TrapExplanation,
};
use gear_core::{
    env::Ext,
    ids::{MessageId, ProgramId, ReservationId},
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gear_core_errors::MemoryError;
use sp_sandbox::{HostError, ReturnValue, Value};

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
    // Issue (#1208)
    val.try_into()
        .map(|v| Value::I64(v).into())
        .map_err(|_| HostError)
}

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum FuncError<E> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[from]
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
    #[display(
        fmt = "Cannot take data by indexes {:?} from message with size {}",
        _0,
        _1
    )]
    ReadWrongRange(Range<usize>, usize),
    #[display(fmt = "Overflow at {} + len {} in `gr_read`", _0, _1)]
    ReadLenOverflow(usize, usize),
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

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
                .send(HandlePacket::new(dest, payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;

            let error_len = ctx
                .ext
                .send(HandlePacket::new_with_gas(dest, payload, gas_limit, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;

            let error_len = ctx
                .ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new(dest, Default::default(), value),
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;

            let error_len = ctx
                .ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit, value),
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let error_len = ctx
                .ext
                .send_init()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|handle| {
                    ctx.write_output(handle_ptr, &handle.to_le_bytes())
                })?;
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

        let mut f = || {
            let payload: Vec<u8> = ctx.read_memory(payload_ptr, payload_len)?;
            let error_len = ctx
                .ext
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

        let at: usize = pop_i32(&mut args)?;
        let len: usize = pop_i32(&mut args)?;
        let dest = pop_i32(&mut args)?;

        ctx.write_validated_output(dest, |ext| {
            let msg = ext.read().map_err(FuncError::Core)?;

            let last_idx = at
                .checked_add(len)
                .ok_or(FuncError::ReadLenOverflow(at, len))?;

            if last_idx > msg.len() {
                return Err(FuncError::ReadWrongRange(at..last_idx, msg.len()));
            }

            Ok(&msg[at..last_idx])
        })
        .map(|()| ReturnValue::Unit)
        .map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn size(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let size = ctx.ext.size().map_err(FuncError::Core);

        match size {
            Ok(size) => return_i32(size),
            Err(err) => {
                ctx.err = err;
                Err(HostError)
            }
        }
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let value_dest_ptr = pop_i32(&mut args.iter())?;

        let mut res = || -> Result<(), _> {
            let value_dest: ProgramId = ctx.read_memory_as(value_dest_ptr)?;
            ctx.ext.exit().map_err(FuncError::Core)?;
            Err(FuncError::Terminated(TerminationReason::Exit(value_dest)))
        };
        if let Err(err) = res() {
            ctx.err = err;
        }

        Err(HostError)
    }

    pub fn exit_code(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let exit_code = ctx.ext.exit_code().map_err(FuncError::Core).map_err(|e| {
            ctx.err = e;
            HostError
        })?;

        if let Some(exit_code) = exit_code {
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
        ctx.alloc(pages)
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

        let mut f = || {
            let origin = ctx.ext.origin().map_err(FuncError::Core)?;
            ctx.write_output(origin_ptr, origin.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit)
            .map_err(|err: FuncError<_>| {
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

        let mut f = || {
            let payload: Vec<u8> = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
                .reply(ReplyPacket::new(payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let payload: Vec<u8> = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let mut value = [0u8; 16];
            ctx.read_memory_into_buf(value_ptr, &mut value)?;
            let value = u128::from_le_bytes(value);
            let error_len = ctx
                .ext
                .reply_commit(ReplyPacket::new(Default::default(), value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let mut f = || {
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
                .reply_commit(ReplyPacket::new_with_gas(
                    Default::default(),
                    gas_limit,
                    value,
                ))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
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

        let message_id = ctx.ext.reply_to().map_err(FuncError::Core).map_err(|err| {
            ctx.err = err;
            HostError
        })?;

        if let Some(id) = message_id {
            ctx.write_output(dest, id.as_ref()).map_err(|err| {
                ctx.err = err.into();
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

        let mut f = || {
            let payload: Vec<u8> = ctx.read_memory(payload_ptr, payload_len)?;
            let error_len = ctx
                .ext
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

        let mut f = || {
            let mut data = vec![0u8; str_len];
            ctx.read_memory_into_buf(str_ptr, &mut data)?;
            let s = String::from_utf8(data).map_err(FuncError::DebugString)?;
            ctx.ext.debug(&s).map_err(FuncError::Core)?;
            Ok(())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let gas_amount: u32 = pop_i32(&mut args)?;
        let id_ptr: u32 = pop_i32(&mut args)?;

        let mut f = || {
            let id = ctx.ext.reserve_gas(gas_amount).map_err(FuncError::Core)?;
            ctx.write_output(id_ptr, id.as_ref())?;
            Ok(())
        };

        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn unreserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let gas_amount: u32 = pop_i32(&mut args)?;
        let id_ptr: u32 = pop_i32(&mut args)?;

        let mut f = || {
            let id: ReservationId = ctx.read_memory_as(id_ptr)?;
            ctx.ext
                .unreserve_gas(id, gas_amount)
                .map_err(FuncError::Core)?;
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

        let mut f = || {
            let message_id = ctx.ext.message_id().map_err(FuncError::Core)?;
            ctx.write_output(msg_id_ptr, message_id.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;

        let mut f = || {
            let program_id = ctx.ext.program_id().map_err(FuncError::Core)?;
            ctx.write_output(program_id_ptr, program_id.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let source_ptr = pop_i32(&mut args)?;

        let res = match ctx.ext.source() {
            Ok(source) => ctx
                .write_output(source_ptr, source.as_ref())
                .map(|()| ReturnValue::Unit)
                .map_err(|err| {
                    ctx.err = err.into();
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

        let mut f = || -> Result<(), FuncError<_>> {
            let value = ctx.ext.value().map_err(FuncError::Core)?;
            ctx.write_output(value_ptr, &value.encode())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;

        let mut f = || {
            let value_available = ctx.ext.value_available().map_err(FuncError::Core)?;
            ctx.write_output(value_ptr, &value_available.encode())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            HostError
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let err = ctx
            .ext
            .leave()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Leave));
        ctx.err = err;
        Err(HostError)
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let err = ctx
            .ext
            .wait()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Wait(None)));
        ctx.err = err;
        Err(HostError)
    }

    pub fn wait_for(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let duration_ptr = pop_i32(&mut args)?;

        let mut f = || {
            let duration: u32 = ctx.read_memory_as(duration_ptr)?;
            ctx.ext.wait_for(duration).map_err(FuncError::Core)?;
            Ok(Some(duration))
        };

        ctx.err = match f() {
            Ok(duration) => FuncError::Terminated(TerminationReason::Wait(duration)),
            Err(e) => e,
        };
        Err(HostError)
    }

    pub fn wait_no_more(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let duration_ptr = pop_i32(&mut args)?;

        let mut f = || {
            let duration: u32 = ctx.read_memory_as(duration_ptr)?;
            ctx.ext.wait_no_more(duration).map_err(FuncError::Core)?;
            Ok(Some(duration))
        };

        ctx.err = match f() {
            Ok(duration) => FuncError::Terminated(TerminationReason::Wait(duration)),
            Err(e) => e,
        };
        Err(HostError)
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let waker_id_ptr = pop_i32(&mut args)?;

        let mut f = || {
            let waker_id: MessageId = ctx.read_memory_as(waker_id_ptr)?;
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

        let mut f = || {
            let code_hash: [u8; 32] = ctx.read_memory_as(code_hash_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
                .create_program(InitPacket::new(code_hash.into(), salt, payload, value))
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|new_actor_id| {
                    ctx.write_output(program_id_ptr, new_actor_id.as_ref())
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

        let mut f = || {
            let code_hash: [u8; 32] = ctx.read_memory_as(code_hash_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let error_len = ctx
                .ext
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
                    ctx.write_output(program_id_ptr, new_actor_id.as_ref())
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

        let mut f = || {
            let err = ctx
                .ext
                .last_error()
                .ok_or(FuncError::SyscallErrorExpected)?;
            let err = err.encode();
            ctx.write_output(data_ptr, &err)?;
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
