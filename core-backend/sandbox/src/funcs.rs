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
use blake2_rfc::blake2b::blake2b;
use core::{convert::TryInto, fmt::Display, marker::PhantomData, ops::Range};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    AsTerminationReason, IntoExtInfo, RuntimeCtx, RuntimeCtxError, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    ids::ReservationId,
    memory::{Memory, PageU32Size, WasmPageNumber},
    message::{HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket},

};
use gear_core_errors::{CoreError, MemoryError};
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthWithCode, LengthWithGas, LengthWithHandle,
    LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use sp_sandbox::{HostError, ReturnValue, Value};

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

#[derive(Debug, derive_more::Display, derive_more::From)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{_0}")]
    Core(E),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeCtx(RuntimeCtxError<E>),
    #[from]
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{_0}")]
    PayloadSize(PayloadSizeError),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "Cannot set u128: {_0}")]
    SetU128(MemoryError),
    #[display(fmt = "Failed to parse debug string: {_0}")]
    DebugString(FromUtf8Error),
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
    #[display(fmt = "Cannot take data by indexes {_0:?} from message with size {_1}")]
    ReadWrongRange(Range<u32>, u32),
    #[display(fmt = "Overflow at {_0} + len {_1} in `gr_read`")]
    ReadLenOverflow(u32, u32),
    #[display(fmt = "Binary code has wrong instrumentation")]
    WrongInstrumentation,
}

impl<E: Display> FuncError<E> {
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

        let (pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .send(HandlePacket::new(destination.into(), payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, payload_ptr, len, gas_limit, delay, err_mid_ptr) =
            args.iter().read_6()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .send(
                    HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;

            ctx.ext
                .send_commit(
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit_wgas, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, gas_limit, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;

            ctx.ext
                .send_commit(
                    handle,
                    HandlePacket::new_with_gas(
                        destination.into(),
                        Default::default(),
                        gas_limit,
                        value,
                    ),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_init, args = {}", args_to_str(args));

        let err_handle_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .send_init()
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_handle_ptr, LengthWithHandle::from(res)))
        })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push, args = {}", args_to_str(args));

        let (handle, payload_ptr, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))?;

            let len = ctx
                .ext
                .send_push(handle, &payload?)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reservation_send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send, args = {}", args_to_str(args));

        let (rid_pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_memory_as(rid_pid_value_ptr)?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .reservation_send(
                    reservation_id.into(),
                    HandlePacket::new(destination.into(), payload, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reservation_send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send_commit, args = {}", args_to_str(args));

        let (handle, rid_pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_memory_as(rid_pid_value_ptr)?;

            ctx.ext
                .reservation_send_commit(
                    reservation_id.into(),
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
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
            .ok_or_else(|| FuncError::ReadLenOverflow(at, len))?;

        if last_idx as usize > msg.len() {
            return Err(FuncError::ReadWrongRange(at..last_idx, msg.len() as u32));
        }

        Ok(&msg[at as usize..last_idx as usize])
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "read, args = {}", args_to_str(args));

        let (at, len, buffer_ptr, err_len_ptr): (_, _, u32, _) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let length = match Self::validated(&mut ctx.ext, at, len) {
                Ok(buffer) => {
                    ctx.memory.write(buffer_ptr, buffer)?;

                    0u32
                }
                // TODO: issue #1652.
                Err(_err) => 1,
            };

            ctx.write_output(err_len_ptr, &length.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn size(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "size, args = {}", args_to_str(args));

        let size_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let size = ctx.ext.size().map_err(FuncError::Core)? as u32;

            ctx.write_output(size_ptr, &size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "exit, args = {}", args_to_str(args));

        let inheritor_id_ptr = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            let inheritor_id = ctx.read_memory_decoded(inheritor_id_ptr)?;

            ctx.ext.exit().map_err(FuncError::Core)?;

            Err(FuncError::Terminated(TerminationReason::Exit(inheritor_id)))
        })
    }

    pub fn status_code(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "status_code, args = {}", args_to_str(args));

        let err_code_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .status_code()
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_code_ptr, LengthWithCode::from(res)))
        })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "alloc, args = {}", args_to_str(args));

        let pages = WasmPageNumber::new(args.iter().read()?).map_err(|_| HostError)?;

        let res = ctx.run_any(|ctx| {
            ctx.alloc(pages)
                .map(|page| {
                    log::debug!("ALLOC: {pages:?} pages at {page:?}");
                    page
                })
                .map_err(Into::into)
        })?;

        Ok(ReturnValue::Value(Value::I32(res.raw() as i32)))
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "free, args = {}", args_to_str(args));

        let page = WasmPageNumber::new(args.iter().read()?).map_err(|_| HostError)?;

        ctx.run(|ctx| {
            ctx.ext
                .free(page)
                .map(|_| log::debug!("FREE: {page:?}"))
                .map_err(|err| {
                    log::debug!("FREE ERROR: {}", err);
                    FuncError::Core(err)
                })
        })
    }

    pub fn block_height(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_height, args = {}", args_to_str(args));

        let height_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let height = ctx.ext.block_height().map_err(FuncError::Core)?;
            ctx.write_output(height_ptr, &height.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_timestamp, args = {}", args_to_str(args));

        let timestamp_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let timestamp = ctx.ext.block_timestamp().map_err(FuncError::Core)?;
            ctx.write_output(timestamp_ptr, &timestamp.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "origin, args = {}", args_to_str(args));

        let origin_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let origin = ctx.ext.origin().map_err(FuncError::Core)?;
            ctx.write_output(origin_ptr, origin.as_ref())
                .map_err(Into::into)
        })
    }

    pub fn random(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "random, args = {}", args_to_str(args));

        let (subject_ptr, bn_random_ptr): (_, _) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let raw_subject: Hash = ctx.read_memory_decoded(subject_ptr)?;

            let (random, bn) = ctx.ext.random().map_err(FuncError::Core)?;
            let subject = [&raw_subject, random].concat();

            let mut hash = [0; 32];
            hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

            ctx.write_memory_as(bn_random_ptr, BlockNumberWithHash { bn, hash })
                .map_err(Into::into)
        })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply, args = {}", args_to_str(args));

        let (payload_ptr, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            ctx.ext
                .reply(ReplyPacket::new(payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_wgas, args = {}", args_to_str(args));

        let (payload_ptr, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            ctx.ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit, args = {}", args_to_str(args));

        let (value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            ctx.ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit_wgas, args = {}", args_to_str(args));

        let (gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            ctx.ext
                .reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reservation_reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply, args = {}", args_to_str(args));

        let (rid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_memory_as(rid_value_ptr)?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .reservation_reply(
                    reservation_id.into(),
                    ReplyPacket::new(payload, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reservation_reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply_commit, args = {}", args_to_str(args));

        let (rid_value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_memory_as(rid_value_ptr)?;

            ctx.ext
                .reservation_reply_commit(
                    reservation_id.into(),
                    ReplyPacket::new(Default::default(), value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_to, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .reply_to()
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn signal_from(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "signal_from, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            ctx.ext
                .signal_from()
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push, args = {}", args_to_str(args));

        let (payload_ptr, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))?;

            let len = ctx
                .ext
                .reply_push(&payload?)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reply_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input, args = {}", args_to_str(args));

        let (offset, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            let push_result = ctx.ext.reply_push_input(offset, len);
            push_result
                .and_then(|_| {
                    ctx.ext
                        .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                })
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn reply_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push_input, args = {}", args_to_str(args));

        let (offset, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let result_len = ctx
                .ext
                .reply_push_input(offset, len)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &result_len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reply_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input_wgas, args = {}", args_to_str(args));

        let (offset, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let value = if value_ptr as i32 == i32::MAX {
                0
            } else {
                ctx.read_memory_decoded(value_ptr)?
            };

            let push_result = ctx.ext.reply_push_input(offset, len);
            push_result
                .and_then(|_| {
                    ctx.ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                })
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;

            let handle = ctx.ext.send_init();
            let push_result =
                handle.and_then(|h| ctx.ext.send_push_input(h, offset, len).map(|_| h));
            push_result
                .and_then(|h| {
                    ctx.ext.send_commit(
                        h,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                })
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn send_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push_input, args = {}", args_to_str(args));

        let (handle, offset, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let result_len = ctx
                .ext
                .send_push_input(handle, offset, len)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &result_len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn send_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, gas_limit, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_memory_as(pid_value_ptr)?;

            let handle = ctx.ext.send_init();
            let push_result =
                handle.and_then(|h| ctx.ext.send_push_input(h, offset, len).map(|_| h));
            push_result
                .and_then(|h| {
                    ctx.ext.send_commit(
                        h,
                        HandlePacket::new_with_gas(
                            destination.into(),
                            Default::default(),
                            gas_limit,
                            value,
                        ),
                        delay,
                    )
                })
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_mid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "debug, args = {}", args_to_str(args));

        let (data_ptr, data_len): (_, u32) = args.iter().read_2()?;

        ctx.run(|ctx| {
            // Todo shall we use Payload here?
            let mut data = RuntimeBuffer::try_new_default(data_len as usize)?;
            ctx.read_memory_into_buf(data_ptr, data.get_mut())?;

            let s = String::from_utf8(data.into_vec()).map_err(FuncError::DebugString)?;
            ctx.ext.debug(&s).map_err(FuncError::Core)?;

            Ok(())
        })
    }

    pub fn reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reserve_gas, args = {}", args_to_str(args));

        let (gas, duration, err_rid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            ctx.ext
                .reserve_gas(gas, duration)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_rid_ptr, LengthWithHash::from(res)))
        })
    }

    pub fn unreserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "unreserve_gas, args = {}", args_to_str(args));

        let (reservation_id_ptr, err_unreserved_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let id: ReservationId = ctx.read_memory_decoded(reservation_id_ptr)?;

            ctx.ext
                .unreserve_gas(id)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| ctx.write_memory_as(err_unreserved_ptr, LengthWithGas::from(res)))
        })
    }

    pub fn system_reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "system_reserve_gas, args = {}", args_to_str(args));

        let (gas, err_len_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let len = ctx
                .ext
                .system_reserve_gas(gas)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "gas_available, args = {}", args_to_str(args));

        let gas_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let gas = ctx.ext.gas_available().map_err(FuncError::Core)?;

            ctx.write_output(gas_ptr, &gas.to_le_bytes())
                .map_err(Into::into)
        })
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
                .unwrap_or_else(|| FuncError::Terminated(TerminationReason::Leave)))
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
                .unwrap_or_else(|| {
                    FuncError::Terminated(TerminationReason::Wait(None, MessageWaitedType::Wait))
                }))
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
                .unwrap_or_else(|| {
                    FuncError::Terminated(TerminationReason::Wait(
                        Some(duration),
                        MessageWaitedType::WaitFor,
                    ))
                }))
        })
    }

    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_up_to, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            Err(FuncError::Terminated(TerminationReason::Wait(
                Some(duration),
                if ctx.ext.wait_up_to(duration).map_err(FuncError::Core)? {
                    MessageWaitedType::WaitUpToFull
                } else {
                    MessageWaitedType::WaitUpTo
                },
            )))
        })
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wake, args = {}", args_to_str(args));

        let (message_id_ptr, delay, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let message_id = ctx.read_memory_decoded(message_id_ptr)?;

            let len = ctx
                .ext
                .wake(message_id, delay)
                .process_error()
                .map_err(FuncError::Core)?
                .error_len();

            ctx.write_output(err_len_ptr, &len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program, args = {}", args_to_str(args));

        let (cid_value_ptr, salt_ptr, salt_len, payload_ptr, payload_len, delay, err_mid_pid_ptr) =
            args.iter().read_7()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_memory_as(cid_value_ptr)?;
            let salt = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(salt_ptr, salt_len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, payload_len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| {
                    ctx.write_memory_as(err_mid_pid_ptr, LengthWithTwoHashes::from(res))
                })
        })
    }

    pub fn create_program_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program_wgas, args = {}", args_to_str(args));

        let (
            cid_value_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            gas_limit,
            delay,
            err_mid_pid_ptr,
        ) = args.iter().read_8()?;

        ctx.run(|ctx| {
            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_memory_as(cid_value_ptr)?;
            let salt = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(salt_ptr, salt_len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;
            let payload = (Payload::max_len() >= (len as usize))
                .then_some(ctx.read_memory(payload_ptr, payload_len))
                .ok_or(FuncError::PayloadSize(PayloadSizeError))??
                .try_into()?;

            ctx.ext
                .create_program(
                    InitPacket::new_with_gas(code_id.into(), salt, payload, gas_limit, value),
                    delay,
                )
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| {
                    ctx.write_memory_as(err_mid_pid_ptr, LengthWithTwoHashes::from(res))
                })
        })
    }

    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "error, args = {}", args_to_str(args));

        // error_bytes_ptr is ptr for buffer of an error
        // err_len_ptr is ptr for len of the error occurred during this syscall
        let (error_bytes_ptr, err_len_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            ctx.ext
                .last_error_encoded()
                .process_error()
                .map_err(FuncError::Core)?
                .proc_res(|res| {
                    let length = match res {
                        Ok(error) => {
                            ctx.write_output(error_bytes_ptr, error.as_ref())?;
                            0
                        }
                        Err(length) => length,
                    };

                    ctx.ext.charge_error().map_err(RuntimeCtxError::Ext)?;
                    ctx.write_output(err_len_ptr, &length.to_le_bytes())
                })
        })
    }

    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "forbidden");

        ctx.run(|_ctx| -> Result<(), _> { Err(FuncError::Core(E::Error::forbidden_function())) })
    }

    pub fn out_of_gas(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_gas");

        ctx.err = FuncError::Core(ctx.ext.out_of_gas());

        Err(HostError)
    }

    pub fn out_of_allowance(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_allowance");

        ctx.ext.out_of_allowance();
        ctx.err = FuncError::Terminated(TerminationReason::GasAllowanceExceeded);

        Err(HostError)
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
    use std::convert::TryFrom;

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
