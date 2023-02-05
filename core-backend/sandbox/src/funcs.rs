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
use alloc::{format, string::String};
use blake2_rfc::blake2b::blake2b;
use codec::Encode;
use core::{convert::TryInto, marker::PhantomData};
use gear_backend_common::{
    memory::{MemoryAccessRecorder, MemoryOwner},
    ActorSyscallFuncError, BackendExt, BackendExtError, BackendState, IntoExtErrorForResult,
    TerminationReason,
};
use gear_core::{
    env::Ext,
    memory::{PageU32Size, WasmPage},
    message::{HandlePacket, InitPacket, MessageWaitedType, ReplyPacket},
};
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthWithCode, LengthWithGas, LengthWithHandle,
    LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use sp_sandbox::{HostError, ReturnValue, Value};

// TODO: change it to u32::MAX (issue #2027)
const PTR_SPECIAL: u32 = i32::MAX as u32;

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

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
    E: BackendExt + 'static,
    E::Error: BackendExtError,
{
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send, args = {}", args_to_str(args));

        let (pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_hash_val = ctx.register_read_as(pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_hash_val)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .send(HandlePacket::new(destination.into(), payload, value), delay)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, payload_ptr, len, gas_limit, delay, err_mid_ptr) =
            args.iter().read_6()?;

        ctx.run(|ctx| {
            let read_hash_val = ctx.register_read_as(pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_hash_val)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .send(
                    HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

            let res = ctx
                .ext
                .send_commit(
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit_wgas, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, gas_limit, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

            let res = ctx
                .ext
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
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_init, args = {}", args_to_str(args));

        let err_handle_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_err_handle = ctx.register_write_as(err_handle_ptr);

            let res = ctx.ext.send_init().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_handle, LengthWithHandle::from(res))?;
            Ok(())
        })
    }

    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push, args = {}", args_to_str(args));

        let (handle, payload_ptr, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let payload = ctx.read(read_payload)?;

            let res = ctx
                .ext
                .send_push(handle, &payload)
                .into_ext_error(&mut ctx.err)?;
            let len = res.err().unwrap_or(0);

            ctx.write_as(write_err_len, len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reservation_send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send, args = {}", args_to_str(args));

        let (rid_pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_as(read_rid_pid_value)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .reservation_send(
                    reservation_id.into(),
                    HandlePacket::new(destination.into(), payload, value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reservation_send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send_commit, args = {}", args_to_str(args));

        let (handle, rid_pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_as(read_rid_pid_value)?;

            let res = ctx
                .ext
                .reservation_send_commit(
                    reservation_id.into(),
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "read, args = {}", args_to_str(args));

        let (at, len, buffer_ptr, err_len_ptr): (_, _, u32, _) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let write_err_len = ctx.register_write_as(err_len_ptr);
            let res = ctx.ext.read(at, len);
            let length = match res.into_ext_error(&mut ctx.err)? {
                Ok(buf) => {
                    let write_buffer = ctx.memory_manager.register_write(buffer_ptr, len);
                    ctx.memory_manager
                        .write(&mut ctx.memory, write_buffer, buf)?;
                    0u32
                }
                Err(err_len) => err_len,
            };

            ctx.write_as(write_err_len, length.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn size(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "size, args = {}", args_to_str(args));

        let size_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_size = ctx.register_write_as(size_ptr);

            let size = ctx.ext.size().map_err(ActorSyscallFuncError::Core)? as u32;

            ctx.write_as(write_size, size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "exit, args = {}", args_to_str(args));

        let inheritor_id_ptr = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);

            let inheritor_id = ctx.read_decoded(read_inheritor_id)?;

            ctx.ext.exit().map_err(ActorSyscallFuncError::Core)?;

            Err(ActorSyscallFuncError::Terminated(TerminationReason::Exit(inheritor_id)).into())
        })
    }

    pub fn status_code(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "status_code, args = {}", args_to_str(args));

        let err_code_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_err_code = ctx.register_write_as(err_code_ptr);

            let res = ctx.ext.status_code().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_code, LengthWithCode::from(res))?;
            Ok(())
        })
    }

    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "alloc, args = {}", args_to_str(args));

        let pages = WasmPage::new(args.iter().read()?).map_err(|_| HostError)?;

        let page = ctx.run_any(|ctx| {
            let page = ctx
                .ext
                .alloc(pages, &mut ctx.memory)
                .map_err(ActorSyscallFuncError::Core)?;
            log::debug!("ALLOC: {pages:?} pages at {page:?}");
            Ok(page)
        })?;

        Ok(ReturnValue::Value(Value::I32(page.raw() as i32)))
    }

    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "free, args = {}", args_to_str(args));

        let page = WasmPage::new(args.iter().read()?).map_err(|_| HostError)?;

        ctx.run(|ctx| {
            ctx.ext.free(page).map_err(ActorSyscallFuncError::Core)?;
            log::debug!("FREE: {page:?}");
            Ok(())
        })
    }

    pub fn block_height(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_height, args = {}", args_to_str(args));

        let height_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_height = ctx.register_write_as(height_ptr);

            let height = ctx
                .ext
                .block_height()
                .map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_height, height.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn block_timestamp(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_timestamp, args = {}", args_to_str(args));

        let timestamp_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_timestamp = ctx.register_write_as(timestamp_ptr);

            let timestamp = ctx
                .ext
                .block_timestamp()
                .map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "origin, args = {}", args_to_str(args));

        let origin_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_origin = ctx.register_write_as(origin_ptr);

            let origin = ctx.ext.origin().map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_origin, origin.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn random(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "random, args = {}", args_to_str(args));

        let (subject_ptr, bn_random_ptr): (_, _) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_subject = ctx.register_read_decoded(subject_ptr);
            let write_bn_random = ctx
                .memory_manager
                .register_write_as::<BlockNumberWithHash>(bn_random_ptr);

            let raw_subject: Hash = ctx.read_decoded(read_subject)?;

            let (random, bn) = ctx.ext.random().map_err(ActorSyscallFuncError::Core)?;
            let subject = [&raw_subject, random].concat();

            let mut hash = [0; 32];
            hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

            ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
                .map_err(Into::into)
        })
    }

    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply, args = {}", args_to_str(args));

        let (payload_ptr, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .reply(ReplyPacket::new(payload, value), delay)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_wgas, args = {}", args_to_str(args));

        let (payload_ptr, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit, args = {}", args_to_str(args));

        let (value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };

            let res = ctx
                .ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit_wgas, args = {}", args_to_str(args));

        let (gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };

            let res = ctx
                .ext
                .reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reservation_reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply, args = {}", args_to_str(args));

        let (rid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_rid_value = ctx.register_read_as(rid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_as(read_rid_value)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .reservation_reply(
                    reservation_id.into(),
                    ReplyPacket::new(payload, value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reservation_reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply_commit, args = {}", args_to_str(args));

        let (rid_value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let read_rid_value = ctx.register_read_as(rid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_as(read_rid_value)?;

            let res = ctx
                .ext
                .reservation_reply_commit(
                    reservation_id.into(),
                    ReplyPacket::new(Default::default(), value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_to, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let res = ctx.ext.reply_to().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn signal_from(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "signal_from, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let res = ctx.ext.signal_from().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push, args = {}", args_to_str(args));

        let (payload_ptr, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let payload = ctx.read(read_payload)?;

            let res = ctx.ext.reply_push(&payload).into_ext_error(&mut ctx.err)?;
            let len = res.err().unwrap_or(0);

            ctx.write_as(write_err_len, len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reply_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input, args = {}", args_to_str(args));

        let (offset, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };

            let mut f = || {
                ctx.ext.reply_push_input(offset, len)?;
                ctx.ext
                    .reply_commit(ReplyPacket::new(Default::default(), value), delay)
            };

            let res = f().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn reply_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push_input, args = {}", args_to_str(args));

        let (offset, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let res = ctx
                .ext
                .reply_push_input(offset, len)
                .into_ext_error(&mut ctx.err)?;
            let len = res.err().unwrap_or(0);

            ctx.write_as(write_err_len, len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn reply_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input_wgas, args = {}", args_to_str(args));

        let (offset, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let value = if value_ptr != PTR_SPECIAL {
                let read_value = ctx.register_read_decoded(value_ptr);
                ctx.read_decoded(read_value)?
            } else {
                0
            };

            let mut f = || {
                ctx.ext.reply_push_input(offset, len)?;
                ctx.ext.reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
            };

            let res = f().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run(|ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

            let mut f = || {
                let handle = ctx.ext.send_init()?;
                ctx.ext.send_push_input(handle, offset, len)?;
                ctx.ext.send_commit(
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
            };
            let res = f().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn send_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push_input, args = {}", args_to_str(args));

        let (handle, offset, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run(|ctx| {
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let result_len = ctx
                .ext
                .send_push_input(handle, offset, len)
                .into_ext_error(&mut ctx.err)?
                .err()
                .unwrap_or(0);

            ctx.write_as(write_err_len, result_len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn send_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, gas_limit, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run(|ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let write_err_mid = ctx.register_write_as(err_mid_ptr);

            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

            let mut f = || {
                let handle = ctx.ext.send_init()?;
                ctx.ext.send_push_input(handle, offset, len)?;
                ctx.ext.send_commit(
                    handle,
                    HandlePacket::new_with_gas(
                        destination.into(),
                        Default::default(),
                        gas_limit,
                        value,
                    ),
                    delay,
                )
            };
            let res = f().into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "debug, args = {}", args_to_str(args));

        let (data_ptr, data_len): (_, u32) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_data = ctx.register_read(data_ptr, data_len);

            let data = ctx.read(read_data)?;

            let s = String::from_utf8(data)?;
            ctx.ext.debug(&s).map_err(ActorSyscallFuncError::Core)?;

            Ok(())
        })
    }

    pub fn reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reserve_gas, args = {}", args_to_str(args));

        let (gas, duration, err_rid_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let write_err_rid = ctx.register_write_as(err_rid_ptr);

            let res = ctx
                .ext
                .reserve_gas(gas, duration)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_rid, LengthWithHash::from(res))?;
            Ok(())
        })
    }

    pub fn unreserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "unreserve_gas, args = {}", args_to_str(args));

        let (reservation_id_ptr, err_unreserved_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
            let write_err_unreserved = ctx.register_write_as(err_unreserved_ptr);

            let id = ctx.read_decoded(read_reservation_id)?;

            let res = ctx.ext.unreserve_gas(id).into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_unreserved, LengthWithGas::from(res))?;
            Ok(())
        })
    }

    pub fn system_reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "system_reserve_gas, args = {}", args_to_str(args));

        let (gas, err_len_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let len = ctx
                .ext
                .system_reserve_gas(gas)
                .into_ext_error(&mut ctx.err)?
                .err()
                .unwrap_or(0);

            ctx.write_as(write_err_len, len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn gas_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "gas_available, args = {}", args_to_str(args));

        let gas_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_gas = ctx.register_write_as(gas_ptr);

            let gas = ctx
                .ext
                .gas_available()
                .map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_gas, gas.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn message_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "message_id, args = {}", args_to_str(args));

        let message_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_message_id = ctx.register_write_as(message_id_ptr);

            let message_id = ctx.ext.message_id().map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_message_id, message_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "program_id, args = {}", args_to_str(args));

        let program_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_program_id = ctx.register_write_as(program_id_ptr);

            let program_id = ctx.ext.program_id().map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_program_id, program_id.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "source, args = {}", args_to_str(args));

        let source_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_source = ctx.register_write_as(source_ptr);

            let source = ctx.ext.source().map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_source, source.into_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_value = ctx.register_write_as(value_ptr);

            let value = ctx.ext.value().map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_value, value.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value_available, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let write_value = ctx.register_write_as(value_ptr);

            let value_available = ctx
                .ext
                .value_available()
                .map_err(ActorSyscallFuncError::Core)?;

            ctx.write_as(write_value, value_available.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "leave");

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .leave()
                .map_err(ActorSyscallFuncError::Core)
                .err()
                .unwrap_or(ActorSyscallFuncError::Terminated(TerminationReason::Leave)))
            .map_err(Into::into)
        })
    }

    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait");

        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .wait()
                .map_err(ActorSyscallFuncError::Core)
                .err()
                .unwrap_or_else(|| {
                    ActorSyscallFuncError::Terminated(TerminationReason::Wait(
                        None,
                        MessageWaitedType::Wait,
                    ))
                }))
            .map_err(Into::into)
        })
    }

    pub fn wait_for(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_for, args = {}", args_to_str(args));

        let duration = args.iter().read()?;
        ctx.run(|ctx| -> Result<(), _> {
            Err(ctx
                .ext
                .wait_for(duration)
                .map_err(ActorSyscallFuncError::Core)
                .err()
                .unwrap_or_else(|| {
                    ActorSyscallFuncError::Terminated(TerminationReason::Wait(
                        Some(duration),
                        MessageWaitedType::WaitFor,
                    ))
                }))
            .map_err(Into::into)
        })
    }

    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_up_to, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            Err(ActorSyscallFuncError::Terminated(TerminationReason::Wait(
                Some(duration),
                if ctx
                    .ext
                    .wait_up_to(duration)
                    .map_err(ActorSyscallFuncError::Core)?
                {
                    MessageWaitedType::WaitUpToFull
                } else {
                    MessageWaitedType::WaitUpTo
                },
            ))
            .into())
        })
    }

    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wake, args = {}", args_to_str(args));

        let (message_id_ptr, delay, err_len_ptr) = args.iter().read_3()?;

        ctx.run(|ctx| {
            let read_message_id = ctx.register_read_decoded(message_id_ptr);
            let write_err_len = ctx.register_write_as(err_len_ptr);

            let message_id = ctx.read_decoded(read_message_id)?;

            let len = ctx
                .ext
                .wake(message_id, delay)
                .into_ext_error(&mut ctx.err)?
                .err()
                .unwrap_or(0);

            ctx.write_as(write_err_len, len.to_le_bytes())
                .map_err(Into::into)
        })
    }

    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program, args = {}", args_to_str(args));

        let (cid_value_ptr, salt_ptr, salt_len, payload_ptr, payload_len, delay, err_mid_pid_ptr) =
            args.iter().read_7()?;

        ctx.run(|ctx| {
            let read_cid_value = ctx.register_read_as(cid_value_ptr);
            let read_salt = ctx.register_read(salt_ptr, salt_len);
            let read_payload = ctx.register_read(payload_ptr, payload_len);
            let write_err_mid_pid = ctx.register_write_as(err_mid_pid_ptr);

            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_as(read_cid_value)?;
            let salt = ctx.read(read_salt)?.try_into()?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res))?;
            Ok(())
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
            let read_cid_value = ctx.register_read_as(cid_value_ptr);
            let read_salt = ctx.register_read(salt_ptr, salt_len);
            let read_payload = ctx.register_read(payload_ptr, payload_len);
            let write_err_mid_pid = ctx.register_write_as(err_mid_pid_ptr);

            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_as(read_cid_value)?;
            let salt = ctx.read(read_salt)?.try_into()?;
            let payload = ctx.read(read_payload)?.try_into()?;

            let res = ctx
                .ext
                .create_program(
                    InitPacket::new_with_gas(code_id.into(), salt, payload, gas_limit, value),
                    delay,
                )
                .into_ext_error(&mut ctx.err)?;
            ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res))?;
            Ok(())
        })
    }

    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "error, args = {}", args_to_str(args));

        // error_bytes_ptr is ptr for buffer of an error
        // err_len_ptr is ptr for len of the error occurred during this syscall
        let (error_bytes_ptr, err_len_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let last_err = ctx.last_err();
            let write_err_len = ctx.register_write_as(err_len_ptr);
            let length: u32 = match last_err {
                Ok(err) => {
                    let err = err.encode();
                    let write_error_bytes = ctx.register_write(error_bytes_ptr, err.len() as u32);
                    ctx.write(write_error_bytes, err.as_ref())?;
                    0
                }
                Err(err) => err.encoded_size() as u32,
            };

            ctx.ext
                .charge_error()
                .map_err(ActorSyscallFuncError::Core)?;
            ctx.write_as(write_err_len, length.to_le_bytes())?;
            Ok(())
        })
    }

    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "forbidden");

        ctx.run(|_ctx| -> Result<(), _> {
            Err(ActorSyscallFuncError::Core(E::Error::forbidden_function()).into())
        })
    }

    pub fn out_of_gas(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_gas");

        ctx.err = ActorSyscallFuncError::Core(ctx.ext.out_of_gas()).into();

        Err(HostError)
    }

    pub fn out_of_allowance(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_allowance");

        ctx.err = ActorSyscallFuncError::Core(ctx.ext.out_of_allowance()).into();

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
