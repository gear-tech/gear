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
use core::{
    convert::{TryFrom, TryInto},
    marker::PhantomData,
    slice::Iter,
};
use gear_backend_common::funcs;
use gear_backend_common::{IntoErrorCode, OnSuccessCode};
use gear_core::env::ExtCarrierWithError;
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
        .map(|v| Value::I32(v).into())
        .map_err(|_| HostError)
}

pub(crate) fn return_i64<T: TryInto<i64>>(val: T) -> SyscallOutput {
    val.try_into()
        .map(|v| Value::I64(v).into())
        .map_err(|_| HostError)
}

fn wto<E>(memory: &mut dyn Memory, ptr: usize, buff: &[u8]) -> Result<(), FuncError<E>> {
    memory.write(ptr, buff).map_err(FuncError::Memory)
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

impl<E: Ext + 'static> FuncsHandler<E> {
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;
        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let dest: ProgramId = funcs::get_bytes32(memory, program_id_ptr)?.into();
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.send(HandlePacket::new(dest, payload, value))
                .map_err(FuncError::Core)
                .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let dest: ProgramId = funcs::get_bytes32(memory, program_id_ptr)?.into();
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.send(HandlePacket::new_with_gas(dest, payload, gas_limit, value))
                .map_err(FuncError::Core)
                .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let dest: ProgramId = funcs::get_bytes32(memory, program_id_ptr)?.into();
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.send_commit(
                handle_ptr,
                HandlePacket::new(dest, Default::default(), value),
            )
            .map_err(FuncError::Core)
            .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let dest: ProgramId = funcs::get_bytes32(memory, program_id_ptr)?.into();
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.send_commit(
                handle_ptr,
                HandlePacket::new_with_gas(dest, Default::default(), gas_limit, value),
            )
            .map_err(FuncError::Core)
            .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let handle_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            ext.send_init()
                .on_success_code(|handle| wto(memory, handle_ptr, &handle.to_le_bytes()))
        })
        .map(|code| Value::I32(code).into())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            Ok(ext
                .send_push(handle_ptr, &payload)
                .map_err(FuncError::Core)
                .into_error_code())
        })
        .map(|code| Value::I32(code).into())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let msg = ext.msg().to_vec();
            wto(memory, dest, &msg[at..(at + len)])
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

        let Runtime { ext, memory, .. } = ctx;

        ctx.trap = ext
            .with_fallible(|ext: &mut E| {
                let value_dest: ProgramId = funcs::get_bytes32(memory, value_dest_ptr)?.into();
                ext.exit(value_dest).map_err(FuncError::Core)
            })
            .err()
            .or_else(|| {
                Some(FuncError::Core(E::Error::from_termination_reason(
                    TerminationReason::Exit,
                )))
            });

        Err(HostError)
    }

    pub fn exit_code(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let reply_tuple = ctx
            .ext
            .with_fallible(|ext| ext.reply_to().map_err(FuncError::Core))
            .map_err(|e| {
                ctx.trap = Some(e);
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
            .with_fallible(|ext| ext.gas(val).map_err(FuncError::Core))
            .map(|()| ReturnValue::Unit)
            .map_err(|e| {
                ctx.trap = Some(e);
                HostError
            })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let pages: u32 = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| ext.alloc(pages.into(), memory).map_err(FuncError::Core))
            .map(|page| {
                log::debug!("ALLOC: {} pages at {:?}", pages, page);
                Value::I32(page.0 as i32).into()
            })
            .map_err(|e| {
                ctx.trap = Some(e);
                HostError
            })
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let page: u32 = pop_i32(&mut args)?;

        if let Err(err) = ctx
            .ext
            .with_fallible(|ext| ext.free(page.into()).map_err(FuncError::Core))
        {
            log::debug!("FREE ERROR: {}", err);
            ctx.trap = Some(err);
            Err(HostError)
        } else {
            log::debug!("FREE: {}", page);
            Ok(ReturnValue::Unit)
        }
    }

    pub fn block_height(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_height = ctx
            .ext
            .with_fallible(|ext| ext.block_height().map_err(FuncError::Core))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;

        return_i32(block_height)
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        let block_timestamp = ctx
            .ext
            .with_fallible(|ext| ext.block_timestamp().map_err(FuncError::Core))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;

        return_i64(block_timestamp)
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let origin_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let origin = ext.origin().map_err(FuncError::Core)?;
            wto(memory, origin_ptr, origin.as_ref())
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
        let message_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.reply(ReplyPacket::new(payload, value))
                .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.reply(ReplyPacket::new_with_gas(payload, gas_limit, value))
                .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.reply_commit(ReplyPacket::new(Default::default(), value))
                .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let gas_limit = pop_i64(&mut args)?;
        let value_ptr = pop_i32(&mut args)?;
        let message_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let value = funcs::get_u128(memory, value_ptr)?;
            ext.reply_commit(ReplyPacket::new_with_gas(
                Default::default(),
                gas_limit,
                value,
            ))
            .on_success_code(|message_id| wto(memory, message_id_ptr, message_id.as_ref()))
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let dest = pop_i32(&mut args)?;

        let maybe_message_id = ctx
            .ext
            .with_fallible(|ext| ext.reply_to().map_err(FuncError::Core))
            .map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;

        if let Some((message_id, _)) = maybe_message_id {
            wto(&mut ctx.memory, dest, message_id.as_ref()).map_err(|err| {
                ctx.trap = Some(err);
                HostError
            })?;

            Ok(ReturnValue::Unit)
        } else {
            ctx.trap = Some(FuncError::NoReplyContext);
            Err(HostError)
        }
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let payload_ptr = pop_i32(&mut args)?;
        let payload_len = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            Ok(ext
                .reply_push(&payload)
                .map_err(FuncError::Core)
                .into_error_code())
        })
        .map(|code| Value::I32(code).into())
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let str_ptr = pop_i32(&mut args)?;
        let str_len = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let mut data = vec![0u8; str_len];
            memory.read(str_ptr, &mut data)?;
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
            .with_fallible(|ext| ext.gas_available().map_err(FuncError::Core))
            .map_err(|_| HostError)?;

        return_i64(gas_available)
    }

    pub fn msg_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let msg_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let message_id = ext.message_id().map_err(FuncError::Core)?;
            wto(memory, msg_id_ptr, message_id.as_ref())
        })
        .map(|()| ReturnValue::Unit)
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let program_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let program_id = ext.program_id().map_err(FuncError::Core)?;
            wto(memory, program_id_ptr, program_id.as_ref())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let source = ext.source().map_err(FuncError::Core)?;
            wto(memory, source_ptr, source.as_ref())
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let value = ext.value().map_err(FuncError::Core)?;
            funcs::set_u128(memory, value_ptr, value).map_err(FuncError::SetU128)
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let value_available = ext.value_available().map_err(FuncError::Core)?;
            funcs::set_u128(memory, value_ptr, value_available).map_err(FuncError::SetU128)
        })
        .map(|()| ReturnValue::Unit)
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.trap = ctx
            .ext
            .with_fallible(|ext| ext.leave().map_err(FuncError::Core))
            .err()
            .or_else(|| {
                Some(FuncError::Core(E::Error::from_termination_reason(
                    TerminationReason::Leave,
                )))
            });
        Err(HostError)
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        ctx.trap = ctx
            .ext
            .with_fallible(|ext| ext.wait().map_err(FuncError::Core))
            .err()
            .or_else(|| {
                Some(FuncError::Core(E::Error::from_termination_reason(
                    TerminationReason::Wait,
                )))
            });
        Err(HostError)
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let mut args = args.iter();

        let waker_id_ptr = pop_i32(&mut args)?;

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext| {
            let waker_id: MessageId = funcs::get_bytes32(memory, waker_id_ptr)?.into();
            ext.wake(waker_id).map_err(FuncError::Core)
        })
        .map(|_| ReturnValue::Unit)
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

        let Runtime { ext, memory, .. } = ctx;

        ext.with_fallible(|ext: &mut E| {
            let code_hash = funcs::get_bytes32(memory, code_hash_ptr)?;
            let salt = funcs::get_vec(memory, salt_ptr, salt_len)?;
            let payload = funcs::get_vec(memory, payload_ptr, payload_len)?;
            let value = funcs::get_u128(memory, value_ptr)?;
            let new_actor_id = ext
                .create_program(InitPacket::new_with_gas(
                    code_hash.into(),
                    salt,
                    payload,
                    gas_limit,
                    value,
                ))
                .map_err(FuncError::Core)?;
            wto(memory, program_id_ptr, new_actor_id.as_ref())
        })
        .map(|()| ReturnValue::Unit)
        .map_err(|err| {
            ctx.trap = Some(err);
            HostError
        })
    }
}
