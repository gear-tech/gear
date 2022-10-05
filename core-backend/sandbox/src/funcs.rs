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
    format,
    string::{FromUtf8Error, String},
};
use core::{
    convert::TryInto,
    fmt::{self, Display},
    marker::PhantomData,
    ops::Range,
};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    AsTerminationReason, IntoExtInfo, RuntimeCtx, RuntimeCtxError, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    message::{HandlePacket, InitPacket, PayloadSizeError, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError};
use sp_sandbox::{HostError, ReturnValue, SandboxMemory, Value};

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[from]
    #[display(fmt = "{}", _0)]
    RuntimeCtx(RuntimeCtxError<E>),
    #[from]
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{}", _0)]
    PayloadSize(PayloadSizeError),
    #[from]
    #[display(fmt = "{}", _0)]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "Cannot set u128: {}", _0)]
    SetU128(MemoryError),
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
    ReadWrongRange(Range<u32>, u32),
    #[display(fmt = "Overflow at {} + len {} in `gr_read`", _0, _1)]
    ReadLenOverflow(u32, u32),
}

impl<E> FuncError<E>
where
    E: fmt::Display,
{
    // fn as_core(&self) -> Option<&E> {
    //     match self {
    //         Self::Core(err) => Some(err),
    //         _ => None,
    //     }
    // }

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

fn args_to_str(args: &[Value]) -> String {
    let mut res = String::new();
    for val in args {
        match val {
            Value::I32(x) => res.push_str(&format!(" I32({:#x}),", *x)),
            Value::I64(x) => res.push_str(&format!(" I64({:#x}),", *x)),
            Value::F32(x) => res.push_str(&format!(" F32({:#x}),", *x)),
            Value::F64(x) => res.push_str(&format!(" F64({:#x}),", *x)),
        }
    }
    res
}

/// We use this macros to avoid perf decrease because of log level comparing.
/// By default `sys-trace` feature is disabled, so this macros does nothing.
/// To see sys-calls tracing enable this feature and rebuild node.
macro_rules! sys_trace {
    (target: $target:expr, $($arg:tt)+) => (
        if cfg!(feature = "sys-trace") {
            log::trace!(target: $target, $($arg)+)
        }
    );
}

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send, args = {}", args_to_str(args));

        let (destination_ptr, payload_ptr, payload_len, value_ptr, delay, message_id_ptr) =
            args.iter().read_6()?;

        ctx.run(|ctx| {
            let destination = ctx.read_memory_as(destination_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .send(HandlePacket::new(destination, payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_wgas, args = {}", args_to_str(args));

        let (
            destination_ptr,
            payload_ptr,
            payload_len,
            gas_limit,
            value_ptr,
            delay,
            message_id_ptr,
        ) = args.iter().read_7()?;

        ctx.run(|ctx| {
            let destination = ctx.read_memory_as(destination_ptr)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .send(
                    HandlePacket::new_with_gas(destination, payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit, args = {}", args_to_str(args));

        let (handle, destination_ptr, value_ptr, delay, message_id_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let destination = ctx.read_memory_as(destination_ptr)?;
            let value = ctx.read_memory_as(value_ptr)?;

            // TODO: CHANGE SEND COMMITS SIGNATURES.
            let handle: u32 = handle;

            ctx.ext
                .send_commit(
                    handle as usize,
                    HandlePacket::new(destination, Default::default(), value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn send_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit_wgas, args = {}", args_to_str(args));

        let (handle, destination_ptr, gas_limit, value_ptr, delay, message_id_ptr) =
            args.iter().read_6()?;

        ctx.run(|ctx| {
            let destination = ctx.read_memory_as(destination_ptr)?;
            let value = ctx.read_memory_as(value_ptr)?;

            // TODO: CHANGE SEND COMMITS SIGNATURES.
            let handle: u32 = handle;

            ctx.ext
                .send_commit(
                    handle as usize,
                    HandlePacket::new_with_gas(destination, Default::default(), gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_init, args = {}", args_to_str(args));

        let handle_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .send_init()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|handle| ctx.write_output(handle_ptr, &handle.to_le_bytes()))
        })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push, args = {}", args_to_str(args));

        let (handle, payload_ptr, payload_len) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let payload = ctx.read_memory(payload_ptr, payload_len)?;

            let handle: u32 = handle;

            Ok(ctx
                .ext
                .send_push(handle as usize, &payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len())
        })
    }

    fn validated(
        ext: &'_ mut E,
        at: u32,
        len: u32,
    ) -> Result<&'_ [u8], FuncError<<E as Ext>::Error>> {
        let msg = ext.read().map_err(FuncError::Core)?;

        let last_idx = at
            .checked_add(len)
            .ok_or(FuncError::ReadLenOverflow(at, len))?;

        if last_idx as usize > msg.len() {
            return Err(FuncError::ReadWrongRange(at..last_idx, msg.len() as u32));
        }

        Ok(&msg[at as usize..last_idx as usize])
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "read, args = {}", args_to_str(args));

        let (at, len, buffer_ptr): (_, _, i32) = args.iter().read_3()?;

        ctx.run(|ctx| {
            match Self::validated(ctx.ext, at, len) {
                Ok(buffer) => {
                    ctx.memory
                        .set(buffer_ptr as u32, buffer)
                        .map_err(|_| MemoryError::OutOfBounds)?;

                    Ok(0u32)
                }
                Err(_err) => Ok(1), // TODO: FIX IT.
            }
        })
    }

    pub fn size(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "size");

        ctx.run(|ctx| Ok(ctx.ext.size().map_err(FuncError::Core)? as u32))
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "exit, args = {}", args_to_str(args));

        let inheritor_id_ptr = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            let inheritor_id = ctx.read_memory_as(inheritor_id_ptr)?;

            ctx.ext.exit().map_err(FuncError::Core)?;

            Err(FuncError::Terminated(TerminationReason::Exit(inheritor_id)))
        })
    }

    pub fn exit_code(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "exit_code, args = {}", args_to_str(args));

        let exit_code_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .exit_code()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|exit_code| {
                    ctx.write_output(exit_code_ptr, exit_code.to_le_bytes().as_ref())
                })
        })
    }

    pub fn gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "gas::gear", "gas, args = {}", args_to_str(args));

        let gas = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext.gas(gas).map_err(|e| {
                if matches!(
                    e.as_termination_reason(),
                    Some(&TerminationReason::GasAllowanceExceeded)
                ) {
                    FuncError::Terminated(TerminationReason::GasAllowanceExceeded)
                } else {
                    FuncError::Core(e)
                }
            })
        })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "alloc, args = {}", args_to_str(args));

        let pages = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.alloc(pages)
                .map(|page| {
                    log::debug!("ALLOC: {} pages at {:?}", pages, page);
                    page.0
                })
                .map_err(Into::into)
        })
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "free, args = {}", args_to_str(args));

        let page: u32 = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .free(page.into())
                .map(|_| log::debug!("FREE: {}", page))
                .map_err(|err| {
                    log::debug!("FREE ERROR: {}", err);
                    FuncError::Core(err)
                })
        })
    }

    pub fn block_height(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_height");

        ctx.run(|ctx| ctx.ext.block_height().map_err(FuncError::Core))
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_timestamp");

        ctx.run(|ctx| ctx.ext.block_timestamp().map_err(FuncError::Core))
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "origin, args = {}", args_to_str(args));

        let origin_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let origin = ctx.ext.origin().map_err(FuncError::Core)?;

            ctx.write_output(origin_ptr, origin.as_ref())?;

            Ok(())
        })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply, args = {}", args_to_str(args));

        let (payload_ptr, payload_len, value_ptr, delay, message_id_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .reply(ReplyPacket::new(payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_wgas, args = {}", args_to_str(args));

        let (payload_ptr, payload_len, gas_limit, value_ptr, delay, message_id_ptr) =
            args.iter().read_6()?;

        ctx.run(|ctx| {
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit, args = {}", args_to_str(args));

        let (value_ptr, delay, message_id_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit_wgas, args = {}", args_to_str(args));

        let (gas_limit, value_ptr, delay, message_id_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|message_id| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())
                })
        })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_to, args = {}", args_to_str(args));

        let reply_to_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .reply_to()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|reply_to| ctx.write_output(reply_to_ptr, reply_to.as_ref()))
        })
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push, args = {}", args_to_str(args));

        let (payload_ptr, payload_len) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let payload = ctx.read_memory(payload_ptr, payload_len)?;

            Ok(ctx
                .ext
                .reply_push(&payload)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len())
        })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "debug, args = {}", args_to_str(args));

        let (data_ptr, data_len) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let data_len: u32 = data_len;

            let mut data = RuntimeBuffer::try_new_default(data_len as usize)?;
            ctx.read_memory_into_buf(data_ptr, data.get_mut())?;

            let s = String::from_utf8(data.into_vec()).map_err(FuncError::DebugString)?;
            ctx.ext.debug(&s).map_err(FuncError::Core)?;

            Ok(())
        })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "gas_available");

        ctx.run(|ctx| ctx.ext.gas_available().map_err(FuncError::Core))
    }

    pub fn message_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "message_id, args = {}", args_to_str(args));

        let message_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let message_id = ctx.ext.message_id().map_err(FuncError::Core)?;

            ctx.write_output(message_id_ptr, message_id.as_ref())
                .map_err(Into::into)
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "program_id, args = {}", args_to_str(args));

        let program_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let program_id = ctx.ext.program_id().map_err(FuncError::Core)?;

            ctx.write_output(program_id_ptr, program_id.as_ref())
                .map_err(Into::into)
        })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "source, args = {}", args_to_str(args));

        let source_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let source = ctx.ext.source().map_err(FuncError::Core)?;

            ctx.write_output(source_ptr, source.as_ref())
                .map_err(Into::into)
        })
    }

    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let value = ctx.ext.value().map_err(FuncError::Core)?;

            ctx.write_output(value_ptr, value.to_le_bytes().as_ref())
                .map_err(Into::into)
        })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value_available, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let value_available = ctx.ext.value_available().map_err(FuncError::Core)?;

            ctx.write_output(value_ptr, value_available.to_le_bytes().as_ref())
                .map_err(Into::into)
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "leave");

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .leave()
                .map_err(FuncError::Core)
                .err()
                .unwrap_or(FuncError::Terminated(TerminationReason::Leave)))
        })
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait");

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .wait()
                .map_err(FuncError::Core)
                .err()
                .unwrap_or(FuncError::Terminated(TerminationReason::Wait(None))))
        })
    }

    pub fn wait_for(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_for, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .wait_for(duration)
                .map_err(FuncError::Core)
                .err()
                .unwrap_or(FuncError::Terminated(TerminationReason::Wait(Some(
                    duration,
                )))))
        })
    }

    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_up_to, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .wait_up_to(duration)
                .map_err(FuncError::Core)
                .err()
                .unwrap_or(FuncError::Terminated(TerminationReason::Wait(Some(
                    duration,
                )))))
        })
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wake, args = {}", args_to_str(args));

        let (message_id_ptr, delay) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let message_id = ctx.read_memory_as(message_id_ptr)?;

            Ok(ctx
                .ext
                .wake(message_id, delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len())
        })
    }

    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program, args = {}", args_to_str(args));

        let (
            code_id_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            value_ptr,
            delay,
            message_id_ptr,
            program_id_ptr,
        ) = args.iter().read_9()?;

        ctx.run(|ctx| {
            let code_id = ctx.read_memory_as(code_id_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .create_program(InitPacket::new(code_id, salt, payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|(message_id, program_id)| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())?;
                    ctx.write_output(program_id_ptr, program_id.as_ref())
                })
        })
    }

    pub fn create_program_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program_wgas, args = {}", args_to_str(args));

        let (
            code_id_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            gas_limit,
            value_ptr,
            delay,
            message_id_ptr,
            program_id_ptr,
        ) = args.iter().read_10()?;

        ctx.run(|ctx| {
            let code_id = ctx.read_memory_as(code_id_ptr)?;
            let salt = ctx.read_memory(salt_ptr, salt_len)?;
            let payload = ctx.read_memory(payload_ptr, payload_len)?.try_into()?;
            let value = ctx.read_memory_as(value_ptr)?;

            ctx.ext
                .create_program(
                    InitPacket::new_with_gas(code_id, salt, payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|(message_id, program_id)| {
                    ctx.write_output(message_id_ptr, message_id.as_ref())?;
                    ctx.write_output(program_id_ptr, program_id.as_ref())
                })
        })
    }

    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> Result<ReturnValue, HostError> {
        sys_trace!(target: "syscall::gear", "error, args = {}", args_to_str(args));

        let buffer_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .last_error_encoded()
                .process_error()
                .map_err(FuncError::Core)?
                .error_len_on_success(|err| ctx.write_output(buffer_ptr, err.as_ref()))
        })
    }

    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "forbidden");

        ctx.run(|_ctx| -> Result<(), _> { Err(FuncError::Core(E::Error::forbidden_function())) })
    }
}

#[allow(clippy::type_complexity)]
pub(crate) trait WasmCompatibleIterator {
    fn read<T: WasmCompatible>(&mut self) -> Result<T, HostError>;

    fn read_2<T1: WasmCompatible, T2: WasmCompatible>(&mut self) -> Result<(T1, T2), HostError> {
        Ok((self.read()?, self.read()?))
    }

    fn read_3<T1: WasmCompatible, T2: WasmCompatible, T3: WasmCompatible>(
        &mut self,
    ) -> Result<(T1, T2, T3), HostError> {
        Ok((self.read()?, self.read()?, self.read()?))
    }

    fn read_4<T1: WasmCompatible, T2: WasmCompatible, T3: WasmCompatible, T4: WasmCompatible>(
        &mut self,
    ) -> Result<(T1, T2, T3, T4), HostError> {
        Ok((self.read()?, self.read()?, self.read()?, self.read()?))
    }

    fn read_5<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }

    fn read_6<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
        T6: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5, T6), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }

    fn read_7<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
        T6: WasmCompatible,
        T7: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5, T6, T7), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }

    fn read_8<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
        T6: WasmCompatible,
        T7: WasmCompatible,
        T8: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5, T6, T7, T8), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }

    fn read_9<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
        T6: WasmCompatible,
        T7: WasmCompatible,
        T8: WasmCompatible,
        T9: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5, T6, T7, T8, T9), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }

    fn read_10<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
        T6: WasmCompatible,
        T7: WasmCompatible,
        T8: WasmCompatible,
        T9: WasmCompatible,
        T10: WasmCompatible,
    >(
        &mut self,
    ) -> Result<(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10), HostError> {
        Ok((
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
            self.read()?,
        ))
    }
}

impl<'a, I: Iterator<Item = &'a Value> + 'a> WasmCompatibleIterator for I {
    fn read<T: WasmCompatible>(&mut self) -> Result<T, HostError> {
        T::from(*self.next().ok_or(HostError)?)
    }
}

pub(crate) trait WasmCompatible: Sized {
    fn from(arg: Value) -> Result<Self, HostError>;

    fn throw_back(self) -> ReturnValue;
}

impl WasmCompatible for () {
    fn from(_arg: Value) -> Result<Self, HostError> {
        Ok(())
    }

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Unit
    }
}

impl WasmCompatible for i32 {
    fn from(arg: Value) -> Result<Self, HostError> {
        if let Value::I32(val) = arg {
            return Ok(val);
        }

        Err(HostError)
    }

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Value(Value::I32(self))
    }
}

impl WasmCompatible for u32 {
    fn from(arg: Value) -> Result<Self, HostError> {
        <i32 as WasmCompatible>::from(arg).map(|v| v as u32)
    }

    fn throw_back(self) -> ReturnValue {
        (self as i32).throw_back()
    }
}

impl WasmCompatible for i64 {
    fn from(arg: Value) -> Result<Self, HostError> {
        if let Value::I64(val) = arg {
            return Ok(val);
        }

        Err(HostError)
    }

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Value(Value::I64(self))
    }
}

impl WasmCompatible for u64 {
    fn from(arg: Value) -> Result<Self, HostError> {
        <i64 as WasmCompatible>::from(arg).map(|v| v as u64)
    }

    fn throw_back(self) -> ReturnValue {
        (self as i64).throw_back()
    }
}

#[test]
fn i32_to_u32_conversion() {
    let i32_var: i32 = 5;
    let u32_var: u32 = 5;

    assert_eq!(i32_var.to_le_bytes(), u32_var.to_le_bytes());
    assert_eq!(i32_var as u32, u32_var);

    let i32_overflow: u32 = u32::try_from(i32::MAX).unwrap() + 1;

    let i32_overflowed: i32 = i32_overflow as i32;

    assert!(u32::try_from(i32_overflowed).is_err());

    let converted: u32 = i32_overflowed as u32;

    assert_eq!(i32_overflow, converted)
}
