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

// use crate::{env::ReturnValue, runtime::Runtime};
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::string::{FromUtf8Error, String};
use codec::Encode;
use core::{
    convert::{TryFrom, TryInto},
    fmt::{self, Display},
    marker::PhantomData,
    ops::Range,
    slice::Iter,
};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    AsTerminationReason, IntoExtInfo, RuntimeCtx, RuntimeCtxError, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    ids::{MessageId, ProgramId},
    message::{HandlePacket, InitPacket, PayloadSizeError, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError};
// use wasmi::{Error, RuntimeValue};
/*
pub(crate) type SyscallOutput<E> = Result<ReturnValue, FuncError<E>>;

pub(crate) fn pop_i32<T: TryFrom<i32>>(arg: &mut Iter<'_, RuntimeValue>) -> Result<T, Error>
where
    <T as TryFrom<i32>>::Error: std::fmt::Display,
{
    match arg.next() {
        Some(RuntimeValue::I32(val)) => Ok((*val)
            .try_into()
            .map_err(|e| Error::Value(format!("{}", e)))?),
        _ => Err(Error::Value("popi32".to_string())),
    }
}

pub(crate) fn pop_i64<T: TryFrom<i64>>(arg: &mut Iter<'_, RuntimeValue>) -> Result<T, Error>
where
    <T as TryFrom<i64>>::Error: std::fmt::Display,
{
    match arg.next() {
        Some(RuntimeValue::I64(val)) => Ok((*val)
            .try_into()
            .map_err(|e| Error::Value(format!("{}", e)))?),
        _ => Err(Error::Value("popi64".to_string())),
    }
}

pub(crate) fn return_i32<T: TryInto<i32>>(val: T) -> Result<ReturnValue, Error> {
    val.try_into()
        .map(|v| RuntimeValue::I32(v).into())
        .map_err(|_| Error::Value("return_i32 err".to_string()))
}

pub(crate) fn return_i64<T: TryInto<i64> + fmt::Display>(val: T) -> Result<ReturnValue, Error> {
    val.try_into()
        .map(|v| RuntimeValue::I64(v).into())
        .map_err(|_| Error::Value("return_i64 err".to_string()))
}*/

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[display(fmt = "Runtime Error")]
    HostError,
    #[from]
    #[display(fmt = "{}", _0)]
    RuntimeCtx(RuntimeCtxError<E>),
    #[from]
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{}", _0)]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[from]
    #[display(fmt = "{}", _0)]
    PayloadSizeLimit(PayloadSizeError),
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

/*
pub struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn send(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .send(HandlePacket::new(dest, payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };

        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let gas_limit = pop_i64(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .send(
                    HandlePacket::new_with_gas(dest, payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new(dest, Default::default(), value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn send_commit_wgas(
        ctx: &mut Runtime<E>,
        args: &[RuntimeValue],
    ) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let gas_limit = pop_i64(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let dest: ProgramId = ctx.read_memory_as(program_id_ptr)?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .send_commit(
                    handle_ptr,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

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
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let error_len = ctx
                .ext
                .send_push(handle_ptr, &payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let at: usize = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let len: usize = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let dest = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

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
            FuncError::HostError
        })
    }

    pub fn size(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let size = ctx.ext.size().map_err(FuncError::Core);

        match size {
            Ok(size) => return_i32(size).map_err(|_| FuncError::HostError),
            Err(err) => {
                ctx.err = err;
                Err(FuncError::HostError)
            }
        }
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let value_dest_ptr = pop_i32(&mut args.iter()).map_err(|_| FuncError::HostError)?;

        let mut res = || -> Result<(), _> {
            let value_dest: ProgramId = ctx.read_memory_as(value_dest_ptr)?;
            ctx.ext.exit().map_err(FuncError::Core)?;
            Err(FuncError::Terminated(TerminationReason::Exit(value_dest)))
        };
        if let Err(err) = res() {
            ctx.err = err;
        }

        Err(FuncError::HostError)
    }

    pub fn exit_code(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let exit_code = ctx.ext.exit_code().map_err(FuncError::Core).map_err(|e| {
            ctx.err = e;
            FuncError::HostError
        })?;

        if let Some(exit_code) = exit_code {
            return_i32(exit_code).map_err(|_| FuncError::HostError)
        } else {
            ctx.err = FuncError::NonReplyExitCode;
            Err(FuncError::HostError)
        }
    }

    pub fn gas(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let val = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        ctx.ext
            .gas(val)
            .map_err(FuncError::Core)
            .map(|()| ReturnValue::Unit)
            .map_err(|e| {
                if let Some(TerminationReason::GasAllowanceExceeded) = e
                    .as_core()
                    .and_then(AsTerminationReason::as_termination_reason)
                {
                    ctx.err = FuncError::Terminated(TerminationReason::GasAllowanceExceeded);
                }
                FuncError::HostError
            })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let pages: u32 = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        ctx.alloc(pages)
            .map(|page| {
                log::debug!("ALLOC: {} pages at {:?}", pages, page);
                RuntimeValue::I32(page.0 as i32).into()
            })
            .map_err(|e| {
                ctx.err = e.into();
                FuncError::HostError
            })
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let page: u32 = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        if let Err(err) = ctx.ext.free(page.into()).map_err(FuncError::Core) {
            log::debug!("FREE ERROR: {}", err);
            ctx.err = err;
            Err(FuncError::HostError)
        } else {
            log::debug!("FREE: {}", page);
            Ok(ReturnValue::Unit)
        }
    }

    pub fn block_height(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let block_height = ctx
            .ext
            .block_height()
            .map_err(FuncError::Core)
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })?;

        return_i32(block_height).map_err(|_| FuncError::HostError)
    }

    pub fn block_timestamp(
        ctx: &mut Runtime<E>,
        _args: &[RuntimeValue],
    ) -> SyscallOutput<E::Error> {
        let block_timestamp =
            ctx.ext
                .block_timestamp()
                .map_err(FuncError::Core)
                .map_err(|err| {
                    ctx.err = err;
                    FuncError::HostError
                })?;

        return_i64(block_timestamp).map_err(|_| FuncError::HostError)
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let origin_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let origin = ctx.ext.origin().map_err(FuncError::Core)?;
            ctx.write_output(origin_ptr, origin.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .reply(ReplyPacket::new(payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let gas_limit = pop_i64(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn reply_commit_wgas(
        ctx: &mut Runtime<E>,
        args: &[RuntimeValue],
    ) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let gas_limit = pop_i64(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let message_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let dest = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let message_id = ctx.ext.reply_to().map_err(FuncError::Core).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })?;

        if let Some(id) = message_id {
            ctx.write_output(dest, id.as_ref()).map_err(|err| {
                ctx.err = err.into();
                FuncError::HostError
            })?;

            Ok(ReturnValue::Unit)
        } else {
            ctx.err = FuncError::NoReplyContext;
            Err(FuncError::HostError)
        }
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let payload = ctx.read_memory(payload_ptr, payload_len)?;
            let error_len = ctx
                .ext
                .reply_push(&payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let str_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let str_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let mut data = RuntimeBuffer::try_new_default(str_len)?;
            ctx.read_memory_into_buf(str_ptr, data.get_mut())?;
            let s = String::from_utf8(data.into_vec()).map_err(FuncError::DebugString)?;
            ctx.ext.debug(&s).map_err(FuncError::Core)?;
            Ok(())
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let gas_available = ctx
            .ext
            .gas_available()
            .map_err(FuncError::Core)
            .map_err(|_| FuncError::HostError)?;

        Ok(return_i64(gas_available).unwrap_or_else(|_| ReturnValue::Value(i64::MAX.into())))
    }

    pub fn msg_id(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let msg_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let message_id = ctx.ext.message_id().map_err(FuncError::Core)?;
            ctx.write_output(msg_id_ptr, message_id.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let program_id = ctx.ext.program_id().map_err(FuncError::Core)?;
            ctx.write_output(program_id_ptr, program_id.as_ref())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let source_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let res = match ctx.ext.source() {
            Ok(source) => ctx
                .write_output(source_ptr, source.as_ref())
                .map(|()| ReturnValue::Unit)
                .map_err(|err| {
                    ctx.err = err.into();
                    FuncError::HostError
                }),
            Err(err) => {
                ctx.err = FuncError::Core(err);
                Err(FuncError::HostError)
            }
        };
        res
    }

    pub fn value(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || -> Result<(), FuncError<_>> {
            let value = ctx.ext.value().map_err(FuncError::Core)?;
            ctx.write_output(value_ptr, &value.encode())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let value_available = ctx.ext.value_available().map_err(FuncError::Core)?;
            ctx.write_output(value_ptr, &value_available.encode())
                .map_err(Into::into)
        };
        f().map(|()| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let err = ctx
            .ext
            .leave()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Leave));
        ctx.err = err;
        Err(FuncError::HostError)
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let err = ctx
            .ext
            .wait()
            .map_err(FuncError::Core)
            .err()
            .unwrap_or(FuncError::Terminated(TerminationReason::Wait(None)));
        ctx.err = err;
        Err(FuncError::HostError)
    }

    pub fn wait_for(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let duration_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let duration: u32 = ctx.read_memory_as(duration_ptr)?;
            ctx.ext.wait_for(duration).map_err(FuncError::Core)?;
            Ok(Some(duration))
        };

        ctx.err = match f() {
            Ok(duration) => FuncError::Terminated(TerminationReason::Wait(duration)),
            Err(e) => e,
        };
        Err(FuncError::HostError)
    }

    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let duration_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let duration: u32 = ctx.read_memory_as(duration_ptr)?;
            ctx.ext.wait_up_to(duration).map_err(FuncError::Core)?;
            Ok(Some(duration))
        };

        ctx.err = match f() {
            Ok(duration) => FuncError::Terminated(TerminationReason::Wait(duration)),
            Err(e) => e,
        };
        Err(FuncError::HostError)
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let waker_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let waker_id: MessageId = ctx.read_memory_as(waker_id_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            ctx.ext.wake(waker_id, delay).map_err(FuncError::Core)
        };
        f().map(|_| ReturnValue::Unit).map_err(|err| {
            ctx.err = err;
            FuncError::HostError
        })
    }

    pub fn create_program(ctx: &mut Runtime<E>, args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let code_hash_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let salt_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let salt_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let code_hash: [u8; 32] = ctx.read_memory_as(code_hash_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .create_program(
                    InitPacket::new(code_hash.into(), salt, payload, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|new_actor_id| {
                    ctx.write_output(program_id_ptr, new_actor_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn create_program_wgas(
        ctx: &mut Runtime<E>,
        args: &[RuntimeValue],
    ) -> SyscallOutput<E::Error> {
        let mut args = args.iter();

        let code_hash_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let salt_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let salt_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let payload_len = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let gas_limit = pop_i64(&mut args).map_err(|_| FuncError::HostError)?;
        let value_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let program_id_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;
        let delay_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

        let mut f = || {
            let code_hash: [u8; 32] = ctx.read_memory_as(code_hash_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value: u128 = ctx.read_memory_as(value_ptr)?;
            let delay: u32 = ctx.read_memory_as(delay_ptr)?;

            let error_len = ctx
                .ext
                .create_program(
                    InitPacket::new_with_gas(code_hash.into(), salt, payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|new_actor_id| {
                    ctx.write_output(program_id_ptr, new_actor_id.as_ref())
                })?;
            Ok(error_len)
        };
        f().map(|code| RuntimeValue::I32(code as i32).into())
            .map_err(|err| {
                ctx.err = err;
                FuncError::HostError
            })
    }

    pub fn error(
        ctx: &mut Runtime<E>,
        args: &[RuntimeValue],
    ) -> Result<ReturnValue, FuncError<E::Error>> {
        let mut args = args.iter();

        let data_ptr = pop_i32(&mut args).map_err(|_| FuncError::HostError)?;

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
            FuncError::HostError
        })
    }

    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[RuntimeValue]) -> SyscallOutput<E::Error> {
        ctx.err = FuncError::Core(E::Error::forbidden_function());
        Err(FuncError::HostError)
    }
}
*/
