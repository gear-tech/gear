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
use alloc::{
    format,
    string::{String, ToString},
};
use blake2_rfc::blake2b::blake2b;
use codec::Encode;
use core::{convert::TryInto, marker::PhantomData};
use gear_backend_common::{
    memory::{MemoryAccessError, MemoryAccessRecorder, MemoryOwner},
    ActorTerminationReason, BackendExt, BackendExtError, BackendState, TrapExplanation,
};
use gear_core::{
    buffer::RuntimeBuffer,
    env::Ext,
    memory::{PageU32Size, WasmPage},
    message::{HandlePacket, InitPacket, MessageWaitedType, ReplyPacket},
};
use gear_core_errors::ExtError;
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthBytes, LengthWithCode, LengthWithGas,
    LengthWithHandle, LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use sp_sandbox::{HostError, ReturnValue, Value};

const PTR_SPECIAL: u32 = u32::MAX;

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
    /// !!! Usage warning: make sure to do it before any other read/write,
    /// because it may contain register read.
    fn register_and_read_value(
        ctx: &mut Runtime<E>,
        value_ptr: u32,
    ) -> Result<u128, MemoryAccessError> {
        if value_ptr != PTR_SPECIAL {
            let read_value = ctx.register_read_decoded(value_ptr);
            return ctx.read_decoded(read_value);
        }

        Ok(0)
    }

    /// Fallible `gr_send` syscall.
    pub fn send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send, args = {}", args_to_str(args));

        let (pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_hash_val = ctx.register_read_as(pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_hash_val)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .send(HandlePacket::new(destination.into(), payload, value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_wgas` syscall.
    pub fn send_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, payload_ptr, len, gas_limit, delay, err_mid_ptr) =
            args.iter().read_6()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_hash_val = ctx.register_read_as(pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_hash_val)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .send(
                    HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_commit` syscall.
    pub fn send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

            ctx.ext
                .send_commit(
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_commit_wgas` syscall.
    pub fn send_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_commit_wgas, args = {}", args_to_str(args));

        let (handle, pid_value_ptr, gas_limit, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
            let HashWithValue {
                hash: destination,
                value,
            } = ctx.read_as(read_pid_value)?;

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
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_init` syscall.
    pub fn send_init(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_init, args = {}", args_to_str(args));

        let err_handle_ptr = args.iter().read()?;

        ctx.run_fallible::<_, _, LengthWithHandle>(err_handle_ptr, |ctx| {
            ctx.ext.send_init().map_err(Into::into)
        })
    }

    /// Fallible `gr_send_push` syscall.
    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push, args = {}", args_to_str(args));

        let (handle, payload_ptr, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let payload = ctx.read(read_payload)?;

            ctx.ext.send_push(handle, &payload).map_err(Into::into)
        })
    }

    /// Fallible `gr_reservation_send` syscall.
    pub fn reservation_send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send, args = {}", args_to_str(args));

        let (rid_pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_as(read_rid_pid_value)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .reservation_send(
                    reservation_id.into(),
                    HandlePacket::new(destination.into(), payload, value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reservation_send_commit` syscall.
    pub fn reservation_send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_send_commit, args = {}", args_to_str(args));

        let (handle, rid_pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
            let TwoHashesWithValue {
                hash1: reservation_id,
                hash2: destination,
                value,
            } = ctx.read_as(read_rid_pid_value)?;

            ctx.ext
                .reservation_send_commit(
                    reservation_id.into(),
                    handle,
                    HandlePacket::new(destination.into(), Default::default(), value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_read` syscall.
    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "read, args = {}", args_to_str(args));

        let (at, len, buffer_ptr, err_len_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            let buffer = ctx.ext.read(at, len)?;

            let write_buffer = ctx.memory_manager.register_write(buffer_ptr, len);
            ctx.memory_manager
                .write(&mut ctx.memory, write_buffer, buffer)
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_size` syscall.
    pub fn size(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "size, args = {}", args_to_str(args));

        let size_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let size = ctx.ext.size()? as u32;

            let write_size = ctx.register_write_as(size_ptr);
            ctx.write_as(write_size, size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_exit` syscall.
    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "exit, args = {}", args_to_str(args));

        let inheritor_id_ptr = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            ctx.ext.exit()?;

            let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);
            let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
            Err(ActorTerminationReason::Exit(inheritor_id).into())
        })
    }

    /// Fallible `gr_status_code` syscall.
    pub fn status_code(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "status_code, args = {}", args_to_str(args));

        let err_code_ptr = args.iter().read()?;

        ctx.run_fallible::<_, _, LengthWithCode>(err_code_ptr, |ctx| {
            ctx.ext.status_code().map_err(Into::into)
        })
    }

    /// Infallible `alloc` syscall.
    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "alloc, args = {}", args_to_str(args));

        let pages = WasmPage::new(args.iter().read()?).map_err(|_| HostError)?;

        let page = ctx.run_any(|ctx| {
            let page = ctx.ext.alloc(pages, &mut ctx.memory)?;

            log::debug!("ALLOC: {pages:?} pages at {page:?}");

            Ok(page)
        })?;

        Ok(ReturnValue::Value(Value::I32(page.raw() as i32)))
    }

    /// Infallible `free` syscall.
    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "free, args = {}", args_to_str(args));

        let page = WasmPage::new(args.iter().read()?).map_err(|_| HostError)?;

        ctx.run(|ctx| {
            ctx.ext.free(page)?;

            log::debug!("FREE: {page:?}");

            Ok(())
        })
    }

    /// Infallible `gr_block_height` syscall.
    pub fn block_height(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_height, args = {}", args_to_str(args));

        let height_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let height = ctx.ext.block_height()?;

            let write_height = ctx.register_write_as(height_ptr);
            ctx.write_as(write_height, height.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_block_timestamp` syscall.
    pub fn block_timestamp(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "block_timestamp, args = {}", args_to_str(args));

        let timestamp_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let timestamp = ctx.ext.block_timestamp()?;

            let write_timestamp = ctx.register_write_as(timestamp_ptr);
            ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_origin` syscall.
    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "origin, args = {}", args_to_str(args));

        let origin_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let origin = ctx.ext.origin()?;

            let write_origin = ctx.register_write_as(origin_ptr);
            ctx.write_as(write_origin, origin.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_random` syscall.
    pub fn random(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "random, args = {}", args_to_str(args));

        let (subject_ptr, bn_random_ptr) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_subject = ctx.register_read_decoded(subject_ptr);
            let write_bn_random = ctx.register_write_as(bn_random_ptr);

            let raw_subject: Hash = ctx.read_decoded(read_subject)?;

            let (random, bn) = ctx.ext.random()?;
            let subject = [&raw_subject, random].concat();

            let mut hash = [0; 32];
            hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

            ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply` syscall.
    pub fn reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply, args = {}", args_to_str(args));

        let (payload_ptr, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let value = Self::register_and_read_value(ctx, value_ptr)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .reply(ReplyPacket::new(payload, value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_wgas` syscall.
    pub fn reply_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_wgas, args = {}", args_to_str(args));

        let (payload_ptr, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let value = Self::register_and_read_value(ctx, value_ptr)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_commit` syscall.
    pub fn reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit, args = {}", args_to_str(args));

        let (value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let value = Self::register_and_read_value(ctx, value_ptr)?;

            ctx.ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_commit_wgas` syscall.
    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_commit_wgas, args = {}", args_to_str(args));

        let (gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let value = Self::register_and_read_value(ctx, value_ptr)?;

            ctx.ext
                .reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reservation_reply` syscall.
    pub fn reservation_reply(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply, args = {}", args_to_str(args));

        let (rid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_rid_value = ctx.register_read_as(rid_value_ptr);
            let read_payload = ctx.register_read(payload_ptr, len);
            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_as(read_rid_value)?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .reservation_reply(
                    reservation_id.into(),
                    ReplyPacket::new(payload, value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reservation_reply_commit` syscall.
    pub fn reservation_reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reservation_reply_commit, args = {}", args_to_str(args));

        let (rid_value_ptr, delay, err_mid_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_rid_value = ctx.register_read_as(rid_value_ptr);
            let HashWithValue {
                hash: reservation_id,
                value,
            } = ctx.read_as(read_rid_value)?;

            ctx.ext
                .reservation_reply_commit(
                    reservation_id.into(),
                    ReplyPacket::new(Default::default(), value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_to` syscall.
    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_to, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            ctx.ext.reply_to().map_err(Into::into)
        })
    }

    /// Fallible `gr_signal_from` syscall.
    pub fn signal_from(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "signal_from, args = {}", args_to_str(args));

        let err_mid_ptr = args.iter().read()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            ctx.ext.signal_from().map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_push` syscall.
    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push, args = {}", args_to_str(args));

        let (payload_ptr, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let payload = ctx.read(read_payload)?;

            ctx.ext.reply_push(&payload).map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_input` syscall.
    pub fn reply_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input, args = {}", args_to_str(args));

        let (offset, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let value = Self::register_and_read_value(ctx, value_ptr)?;

            let mut f = || {
                ctx.ext.reply_push_input(offset, len)?;
                ctx.ext
                    .reply_commit(ReplyPacket::new(Default::default(), value), delay)
            };

            f().map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_push_input` syscall.
    pub fn reply_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_push_input, args = {}", args_to_str(args));

        let (offset, len, err_len_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            ctx.ext.reply_push_input(offset, len).map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_input_wgas` syscall.
    pub fn reply_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reply_input_wgas, args = {}", args_to_str(args));

        let (offset, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let value = Self::register_and_read_value(ctx, value_ptr)?;

            let mut f = || {
                ctx.ext.reply_push_input(offset, len)?;
                ctx.ext.reply_commit(
                    ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                    delay,
                )
            };

            f().map_err(Into::into)
        })
    }

    /// Fallible `gr_send_input` syscall.
    pub fn send_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, delay, err_mid_ptr) = args.iter().read_5()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
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

            f().map_err(Into::into)
        })
    }

    /// Fallible `gr_send_push_input` syscall.
    pub fn send_push_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_push_input, args = {}", args_to_str(args));

        let (handle, offset, len, err_len_ptr) = args.iter().read_4()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            ctx.ext
                .send_push_input(handle, offset, len)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_push_input_wgas` syscall.
    pub fn send_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "send_input_wgas, args = {}", args_to_str(args));

        let (pid_value_ptr, offset, len, gas_limit, delay, err_mid_ptr) = args.iter().read_6()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, |ctx| {
            let read_pid_value = ctx.register_read_as(pid_value_ptr);
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

            f().map_err(Into::into)
        })
    }

    /// Infallible `gr_debug` syscall.
    pub fn debug(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "debug, args = {}", args_to_str(args));

        let (data_ptr, data_len): (_, u32) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_data = ctx.register_read(data_ptr, data_len);
            let data: RuntimeBuffer = ctx.read(read_data)?.try_into()?;

            let s = String::from_utf8(data.into_vec())?;
            ctx.ext.debug(&s)?;

            Ok(())
        })
    }

    /// Infallible `gr_panic` syscall.
    pub fn panic(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "panic, args = {}", args_to_str(args));

        let (data_ptr, data_len): (_, u32) = args.iter().read_2()?;

        ctx.run(|ctx| {
            let read_data = ctx.register_read(data_ptr, data_len);
            let data = ctx.read(read_data).unwrap_or_default();

            let s = String::from_utf8_lossy(&data).to_string();

            Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
        })
    }

    /// Fallible `gr_reserve_gas` syscall.
    pub fn reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "reserve_gas, args = {}", args_to_str(args));

        let (gas, duration, err_rid_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthWithHash>(err_rid_ptr, |ctx| {
            ctx.ext.reserve_gas(gas, duration).map_err(Into::into)
        })
    }

    /// Fallible `gr_unreserve_gas` syscall.
    pub fn unreserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "unreserve_gas, args = {}", args_to_str(args));

        let (reservation_id_ptr, err_unreserved_ptr) = args.iter().read_2()?;

        ctx.run_fallible::<_, _, LengthWithGas>(err_unreserved_ptr, |ctx| {
            let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
            let reservation_id = ctx.read_decoded(read_reservation_id)?;

            ctx.ext.unreserve_gas(reservation_id).map_err(Into::into)
        })
    }

    /// Fallible `gr_system_reserve_gas` syscall.
    pub fn system_reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "system_reserve_gas, args = {}", args_to_str(args));

        let (gas, err_len_ptr) = args.iter().read_2()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            ctx.ext.system_reserve_gas(gas).map_err(Into::into)
        })
    }

    /// Infallible `gr_gas_available` syscall.
    pub fn gas_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "gas_available, args = {}", args_to_str(args));

        let gas_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let gas = ctx.ext.gas_available()?;

            let write_gas = ctx.register_write_as(gas_ptr);
            ctx.write_as(write_gas, gas.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_message_id` syscall.
    pub fn message_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "message_id, args = {}", args_to_str(args));

        let message_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let message_id = ctx.ext.message_id()?;

            let write_message_id = ctx.register_write_as(message_id_ptr);
            ctx.write_as(write_message_id, message_id.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_program_id` syscall.
    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "program_id, args = {}", args_to_str(args));

        let program_id_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let program_id = ctx.ext.program_id()?;

            let write_program_id = ctx.register_write_as(program_id_ptr);
            ctx.write_as(write_program_id, program_id.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_source` syscall.
    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "source, args = {}", args_to_str(args));

        let source_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let source = ctx.ext.source()?;

            let write_source = ctx.register_write_as(source_ptr);
            ctx.write_as(write_source, source.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_value` syscall.
    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let value = ctx.ext.value()?;

            let write_value = ctx.register_write_as(value_ptr);
            ctx.write_as(write_value, value.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_value_available` syscall.
    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "value_available, args = {}", args_to_str(args));

        let value_ptr = args.iter().read()?;

        ctx.run(|ctx| {
            let value_available = ctx.ext.value_available()?;

            let write_value = ctx.register_write_as(value_ptr);
            ctx.write_as(write_value, value_available.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_leave` syscall.
    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "leave");

        ctx.run(|ctx| {
            ctx.ext.leave()?;
            Err(ActorTerminationReason::Leave.into())
        })
    }

    /// Infallible `gr_wait` syscall.
    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait");

        ctx.run(|ctx| -> Result<(), _> {
            ctx.ext.wait()?;
            Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
        })
    }

    /// Infallible `gr_wait_for` syscall.
    pub fn wait_for(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_for, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            ctx.ext.wait_for(duration)?;
            Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
        })
    }

    /// Infallible `gr_wait_up_to` syscall.
    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wait_up_to, args = {}", args_to_str(args));

        let duration = args.iter().read()?;

        ctx.run(|ctx| -> Result<(), _> {
            let waited_type = if ctx.ext.wait_up_to(duration)? {
                MessageWaitedType::WaitUpToFull
            } else {
                MessageWaitedType::WaitUpTo
            };
            Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
        })
    }

    /// Fallible `gr_wake` syscall.
    pub fn wake(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "wake, args = {}", args_to_str(args));

        let (message_id_ptr, delay, err_len_ptr) = args.iter().read_3()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            let read_message_id = ctx.register_read_decoded(message_id_ptr);
            let message_id = ctx.read_decoded(read_message_id)?;

            ctx.ext.wake(message_id, delay).map_err(Into::into)
        })
    }

    /// Fallible `gr_create_program` syscall.
    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "create_program, args = {}", args_to_str(args));

        let (cid_value_ptr, salt_ptr, salt_len, payload_ptr, payload_len, delay, err_mid_pid_ptr) =
            args.iter().read_7()?;

        ctx.run_fallible::<_, _, LengthWithTwoHashes>(err_mid_pid_ptr, |ctx| {
            let read_cid_value = ctx.register_read_as(cid_value_ptr);
            let read_salt = ctx.register_read(salt_ptr, salt_len);
            let read_payload = ctx.register_read(payload_ptr, payload_len);
            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_as(read_cid_value)?;
            let salt = ctx.read(read_salt)?.try_into()?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_create_program_wgas` syscall.
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

        ctx.run_fallible::<_, _, LengthWithTwoHashes>(err_mid_pid_ptr, |ctx| {
            let read_cid_value = ctx.register_read_as(cid_value_ptr);
            let read_salt = ctx.register_read(salt_ptr, salt_len);
            let read_payload = ctx.register_read(payload_ptr, payload_len);
            let HashWithValue {
                hash: code_id,
                value,
            } = ctx.read_as(read_cid_value)?;
            let salt = ctx.read(read_salt)?.try_into()?;
            let payload = ctx.read(read_payload)?.try_into()?;

            ctx.ext
                .create_program(
                    InitPacket::new_with_gas(code_id.into(), salt, payload, gas_limit, value),
                    delay,
                )
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_error` syscall.
    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "error, args = {}", args_to_str(args));

        // `error_bytes_ptr` is ptr for buffer of an error
        // `err_len_ptr` is ptr for len of the error occurred during this syscall
        let (error_bytes_ptr, err_len_ptr) = args.iter().read_2()?;

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, |ctx| {
            ctx.ext.charge_error()?;

            if let Some(err) = ctx.fallible_syscall_error.as_ref() {
                let err = err.encode();
                let write_error_bytes = ctx.register_write(error_bytes_ptr, err.len() as u32);
                ctx.write(write_error_bytes, err.as_ref())?;
                Ok(())
            } else {
                Err(
                    ActorTerminationReason::Trap(TrapExplanation::Ext(ExtError::SyscallUsage))
                        .into(),
                )
            }
        })
    }

    /// Infallible `forbidden` syscall-placeholder.
    pub fn forbidden(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "forbidden");

        ctx.run(|_| Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into()))
    }

    /// Infallible `gr_out_of_gas` syscall.
    pub fn out_of_gas(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_gas");

        let reason = ctx.ext.out_of_gas().into_termination_reason();
        ctx.set_termination_reason(reason);

        Err(HostError)
    }

    /// Infallible `gr_out_of_allowance` syscall.
    pub fn out_of_allowance(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        sys_trace!(target: "syscall::gear", "out_of_allowance");

        let reason = ctx.ext.out_of_allowance().into_termination_reason();
        ctx.set_termination_reason(reason);

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
