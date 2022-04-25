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
use alloc::string::FromUtf8Error;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{string::String, vec};
use core::fmt;
use core::{
    convert::{TryFrom, TryInto},
    marker::PhantomData,
    slice::Iter,
};
use gear_backend_common::funcs;
use gear_backend_common::{IntoErrorCode, OnSuccessCode};
use gear_core::env::LaterExtWithError;
use gear_core::{
    env::Ext,
    ids::{MessageId, ProgramId},
    memory::Memory,
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError, TerminationReason};
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
        .map(|v| ReturnValue::Value(Value::I32(v)))
        .map_err(|_| HostError)
}

pub(crate) fn return_i64<T: TryInto<i64>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| ReturnValue::Value(Value::I64(v)))
        .map_err(|_| HostError)
}

fn wto<E: Ext>(ctx: &mut Runtime<E>, ptr: usize, buff: &[u8]) -> Result<(), FuncError<E::Error>> {
    ctx.memory.write(ptr, buff).map_err(FuncError::Memory)
}

pub enum FuncError<E> {
    Core(E),
    LaterExtWith(LaterExtWithError),
    Memory(MemoryError),
    SetU128(MemoryError),
    Exit,
    NonReplyExitCode,
    GasUsageReport,
    NoReplyContext,
    DebugString(FromUtf8Error),
}

impl<E> From<LaterExtWithError> for FuncError<E> {
    fn from(err: LaterExtWithError) -> Self {
        Self::LaterExtWith(err)
    }
}

impl<E> From<MemoryError> for FuncError<E> {
    fn from(err: MemoryError) -> Self {
        Self::Memory(err)
    }
}

impl<E> fmt::Display for FuncError<E>
where
    E: CoreError,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FuncError::Core(err) => fmt::Display::fmt(err, f),
            FuncError::LaterExtWith(err) => fmt::Display::fmt(err, f),
            FuncError::Memory(err) => fmt::Display::fmt(err, f),
            FuncError::SetU128(err) => write!(f, "Cannot set u128: {}", err),
            FuncError::Exit => write!(f, "`gr_exit` has been called"),
            FuncError::NonReplyExitCode => write!(f, "Exit code ran into non-reply scenario"),
            FuncError::GasUsageReport => write!(f, "Unable to report about gas used"),
            FuncError::NoReplyContext => write!(f, "Not running in reply context"),
            FuncError::DebugString(err) => write!(f, "Failed to parse debug string: {}", err),
        }
    }
}

pub(crate) struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

impl<E: Ext + 'static> FuncsHandler<E> {
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let dest: ProgramId = funcs::get_bytes32(&ctx.memory, program_id_ptr)?.into();
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                ext.send(HandlePacket::new(dest, payload, value))
                    .map_err(FuncError::Core)
                    .on_success_code(|message_id| wto(ctx, message_id_ptr, message_id.as_ref()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
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

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let dest: ProgramId = funcs::get_bytes32(&ctx.memory, program_id_ptr)?.into();
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                ext.send(HandlePacket::new_with_gas(dest, payload, gas_limit, value))
                    .map_err(FuncError::Core)
                    .on_success_code(|message_id| wto(ctx, message_id_ptr, message_id.as_ref()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let dest: ProgramId = funcs::get_bytes32(&ctx.memory, program_id_ptr)?.into();
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                ext.send_commit(
                    handle_ptr,
                    HandlePacket::new(dest, Default::default(), value),
                )
                .map_err(FuncError::Core)
                .on_success_code(|message_id| wto(ctx, message_id_ptr, message_id.as_ref()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
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

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let dest: ProgramId = funcs::get_bytes32(&ctx.memory, program_id_ptr)?.into();
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                ext.send_commit(
                    handle_ptr,
                    HandlePacket::new_with_gas(dest, Default::default(), gas_limit, value),
                )
                .map_err(FuncError::Core)
                .on_success_code(|message_id| wto(ctx, message_id_ptr, message_id.as_ref()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                ext.send_init()
                    .map_err(FuncError::Core)
                    .on_success_code(|handle| wto(ctx, handle_ptr, &handle.to_le_bytes()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;

        ctx.ext
            .with_fallible(|ext| {
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                Ok(ext
                    .send_push(handle_ptr, &payload)
                    .map_err(FuncError::Core)
                    .into_error_code())
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let at = pop_i32(&mut args)?;
        let len: usize = pop_i32(&mut args)?;
        let dest = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let msg = ext.msg().to_vec();
                wto(ctx, dest, &msg[at..(at + len)])
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn size(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.ext
            .with(|ext| ext.msg().len())
            .map(return_i32)
            .unwrap_or_else(|_| return_i32(0))
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let value_dest_ptr = pop_i32(&mut args.iter())?;

        ctx.ext
            .with_fallible(|ext: &mut E| {
                let value_dest: ProgramId = funcs::get_bytes32(&ctx.memory, value_dest_ptr)?.into();
                ext.exit(value_dest).map_err(FuncError::Core)
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;

        ctx.trap = Some(FuncError::Exit);
        Err(HostError)
    }

    pub fn exit_code(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let reply_tuple = ctx.ext.with(|ext| ext.reply_to()).map_err(|err| {
            ctx.trap = Some(err.into());
            HostError
        })?;

        if let Some((_, exit_code)) = reply_tuple {
            return_i32(exit_code)
        } else {
            ctx.trap = Some(FuncError::NonReplyExitCode);
            Err(HostError)
        }
    }

    pub fn gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let val = pop_i32(&mut args)?;

        ctx.ext
            .with_fallible(|ext| ext.charge_gas(val).map_err(FuncError::Core))
            .map(|()| ReturnValue::Unit)
            .map_err(|e| {
                ctx.trap = match e {
                    FuncError::Core(e)
                        // TODO: fix incorrect logic
                        if e.as_termination_reason() != Some(TerminationReason::GasAllowance) =>
                    {
                        Some(FuncError::<E::Error>::GasUsageReport)
                    }
                    e => Some(e),
                };

                HostError
            })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let pages: u32 = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                ext.alloc(pages.into(), &mut ctx.memory)
                    .map_err(FuncError::Core)
            })
            .map(|page| {
                log::debug!("ALLOC: {} pages at {:?}", pages, page);
                ReturnValue::Value(Value::I32(page.0 as i32))
            })
            .map_err(|e| {
                ctx.trap = Some(e);
                HostError
            })
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let page: u32 = pop_i32(&mut args)?;

        if let Err(e) = ctx
            .ext
            .with_fallible(|ext| ext.free(page.into()).map_err(FuncError::Core))
        {
            log::debug!("FREE ERROR: {}", e);
        } else {
            log::debug!("FREE: {}", page);
        }

        Ok(ReturnValue::Unit)
    }

    pub fn block_height(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_height = ctx
            .ext
            .with(|ext| ext.block_height())
            .map_err(|_| HostError)?;

        return_i32(block_height)
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_timestamp = ctx
            .ext
            .with(|ext| ext.block_timestamp())
            .map_err(|_| HostError)?;

        return_i64(block_timestamp)
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let origin_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let origin = ext.origin();
                wto(ctx, origin_ptr, origin.as_ref())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;

        ctx.ext
            .with_fallible(|ext| {
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                Ok(ext
                    .reply(ReplyPacket::new(payload, value))
                    .map(|_| ())
                    .into_error_code())
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let message_id_ptr = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                ext.reply_commit(ReplyPacket::new(Default::default(), value))
                    .map_err(FuncError::Core)
                    .on_success_code(|message_id| wto(ctx, message_id_ptr, message_id.as_ref()))
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let dest = pop_i32(&mut args)?;

        let maybe_message_id = ctx.ext.with(|ext| ext.reply_to()).map_err(|err| {
            ctx.trap = Some(err.into());
            HostError
        })?;

        match maybe_message_id {
            Some((message_id, _)) => wto(ctx, dest, message_id.as_ref()).map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?,
            None => {
                ctx.trap = Some(FuncError::NoReplyContext);
                return Err(HostError);
            }
        };

        Ok(ReturnValue::Unit)
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;

        ctx.ext
            .with_fallible(|ext| {
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                Ok(ext
                    .reply_push(&payload)
                    .map_err(FuncError::Core)
                    .into_error_code())
            })
            .map(|code| ReturnValue::Value(Value::I32(code)))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let str_ptr = pop_i32(&mut args)?;
        let str_len = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let mut data = vec![0u8; str_len];
                ctx.memory.read(str_ptr, &mut data)?;
                let s = String::from_utf8(data).map_err(FuncError::DebugString)?;
                ext.debug(&s).map_err(FuncError::Core)?;
                Ok(())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let gas_available = ctx
            .ext
            .with(|ext| ext.gas_available())
            .map_err(|_| HostError)?;

        return_i64(gas_available)
    }

    pub fn msg_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let msg_id_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let message_id = ext.message_id();
                wto(ctx, msg_id_ptr, message_id.as_ref())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let source_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let source = ext.program_id();
                wto(ctx, source_ptr, source.as_ref())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let source_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                let source = ext.source();
                wto(ctx, source_ptr, source.as_ref())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                funcs::set_u128(&mut ctx.memory, value_ptr, ext.value()).map_err(FuncError::SetU128)
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext| {
                funcs::set_u128(&mut ctx.memory, value_ptr, ext.value_available())
                    .map_err(FuncError::SetU128)
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.ext
            .with_fallible(|ext| ext.leave().map_err(FuncError::Core))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;
        ctx.trap = Some(FuncError::Core(E::Error::from_termination_reason(
            TerminationReason::Leave,
        )));
        Err(HostError)
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.ext
            .with_fallible(|ext| ext.wait().map_err(FuncError::Core))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;
        ctx.trap = Some(FuncError::Core(E::Error::from_termination_reason(
            TerminationReason::Wait,
        )));
        Err(HostError)
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let waker_id_ptr = pop_i32(&mut args)?;

        ctx.ext
            .with_fallible(|ext| {
                let waker_id: MessageId = funcs::get_bytes32(&ctx.memory, waker_id_ptr)?.into();
                ext.wake(waker_id).map_err(FuncError::Core)
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }

    pub fn create_program_wgas(
        ctx: &mut Runtime<E>,
        args: &[Value],
    ) -> Result<ReturnValue, HostError> {
        let mut args = args.iter();

        let code_hash_ptr = pop_i32(&mut args)?;
        let salt_ptr = pop_i32(&mut args)?;
        let salt_len = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let program_id_ptr = pop_i32(&mut args)?;

        ctx.ext
            .clone()
            .with_fallible(|ext: &mut E| {
                let code_hash = funcs::get_bytes32(&ctx.memory, code_hash_ptr)?;
                let salt = funcs::get_vec(&ctx.memory, salt_ptr, salt_len)?;
                let payload = funcs::get_vec(&ctx.memory, payload_ptr, payload_len)?;
                let value = funcs::get_u128(&ctx.memory, value_ptr)?;
                let new_actor_id = ext
                    .create_program(InitPacket::new_with_gas(
                        code_hash.into(),
                        salt,
                        payload,
                        gas_limit,
                        value,
                    ))
                    .map_err(FuncError::Core)?;
                wto(ctx, program_id_ptr, new_actor_id.as_ref())
            })
            .map(|()| ReturnValue::Unit)
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })
    }
}
