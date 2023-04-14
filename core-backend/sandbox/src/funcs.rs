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
use alloc::string::{String, ToString};
use blake2_rfc::blake2b::blake2b;
use codec::Encode;
use core::{convert::TryInto, marker::PhantomData};
use gear_backend_common::{
    memory::{MemoryAccessError, MemoryAccessRecorder, MemoryOwner},
    syscall_trace, ActorTerminationReason, BackendAllocExtError, BackendExt, BackendExtError,
    BackendState, TrapExplanation, PTR_SPECIAL,
};
use gear_core::{
    buffer::RuntimeBuffer,
    costs::RuntimeCosts,
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

pub(crate) type SyscallOutput = Result<ReturnValue, HostError>;

pub(crate) struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

impl<E> FuncsHandler<E>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
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
        let (pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!("send", pid_value_ptr, payload_ptr, len, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Send(len), |ctx| {
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
        let (pid_value_ptr, payload_ptr, len, gas_limit, delay, err_mid_ptr) = args.iter().read_6();

        syscall_trace!(
            "send_wgas",
            pid_value_ptr,
            payload_ptr,
            len,
            gas_limit,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Send(len), |ctx| {
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
        let (handle, pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4();

        syscall_trace!("send_commit", handle, pid_value_ptr, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendCommit(0), |ctx| {
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
        let (handle, pid_value_ptr, gas_limit, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!(
            "send_commit_wgas",
            handle,
            pid_value_ptr,
            gas_limit,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendCommit(0), |ctx| {
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
        let err_handle_ptr = args.iter().read();

        syscall_trace!("send_init", err_handle_ptr);

        ctx.run_fallible::<_, _, LengthWithHandle>(err_handle_ptr, RuntimeCosts::SendInit, |ctx| {
            ctx.ext.send_init().map_err(Into::into)
        })
    }

    /// Fallible `gr_send_push` syscall.
    pub fn send_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (handle, payload_ptr, len, err_len_ptr) = args.iter().read_4();

        syscall_trace!("send_push", handle, payload_ptr, len, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::SendPush(len), |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let payload = ctx.read(read_payload)?;

            ctx.ext.send_push(handle, &payload).map_err(Into::into)
        })
    }

    /// Fallible `gr_reservation_send` syscall.
    pub fn reservation_send(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (rid_pid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!(
            "reservation_send",
            rid_pid_value_ptr,
            payload_ptr,
            len,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(
            err_mid_ptr,
            RuntimeCosts::ReservationSend(len),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_reservation_send_commit` syscall.
    pub fn reservation_send_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (handle, rid_pid_value_ptr, delay, err_mid_ptr) = args.iter().read_4();

        syscall_trace!(
            "reservation_send_commit",
            handle,
            rid_pid_value_ptr,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(
            err_mid_ptr,
            RuntimeCosts::ReservationSendCommit(0),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_read` syscall.
    pub fn read(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (at, len, buffer_ptr, err_len_ptr) = args.iter().read_4();

        syscall_trace!("read", at, len, buffer_ptr, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::Read, |ctx| {
            // Here `ctx.ext` is const borrowed, so we cannot use `ctx` mut methods.
            let (buffer, mut gas_left) = ctx.ext.read(at, len)?;

            let write_buffer = ctx.memory_manager.register_write(buffer_ptr, len);
            ctx.memory_manager
                .write(&mut ctx.memory, write_buffer, buffer, &mut gas_left)?;
            ctx.ext.set_gas_left(gas_left);
            Ok(())
        })
    }

    /// Infallible `gr_size` syscall.
    pub fn size(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let size_ptr = args.iter().read();

        syscall_trace!("size", size_ptr);

        ctx.run(RuntimeCosts::Size, |ctx| {
            let size = ctx.ext.size()? as u32;

            let write_size = ctx.register_write_as(size_ptr);
            ctx.write_as(write_size, size.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_exit` syscall.
    pub fn exit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let inheritor_id_ptr = args.iter().read();

        syscall_trace!("exit", inheritor_id_ptr);

        ctx.run(RuntimeCosts::Exit, |ctx| -> Result<(), _> {
            let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);
            let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
            Err(ActorTerminationReason::Exit(inheritor_id).into())
        })
    }

    /// Fallible `gr_status_code` syscall.
    pub fn status_code(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let err_code_ptr = args.iter().read();

        syscall_trace!("status_code", err_code_ptr);

        ctx.run_fallible::<_, _, LengthWithCode>(err_code_ptr, RuntimeCosts::StatusCode, |ctx| {
            ctx.ext.status_code().map_err(Into::into)
        })
    }

    /// Infallible `alloc` syscall.
    pub fn alloc(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let pages = args.iter().read();

        syscall_trace!("alloc", pages);

        ctx.run_any(RuntimeCosts::Alloc, |ctx| {
            // TODO: return u32::MAX in case this is error #2353
            let pages = WasmPage::new(pages).map_err(|_| TrapExplanation::Unknown)?;

            let res = ctx.ext.alloc(pages, &mut ctx.memory);
            let res = ctx.process_alloc_func_result(res)?;
            let page = match res {
                Ok(page) => {
                    log::trace!("Alloc {pages:?} pages at {page:?}");
                    page.raw()
                }
                Err(err) => {
                    log::trace!("Alloc failed: {err}");
                    u32::MAX
                }
            };
            Ok(ReturnValue::Value(Value::I32(page as i32)))
        })
    }

    /// Infallible `free` syscall.
    pub fn free(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let page_no = args.iter().read();

        syscall_trace!("free", page_no);

        let page = WasmPage::new(page_no).map_err(|_| HostError)?;

        ctx.run_any(RuntimeCosts::Free, |ctx| {
            let res = ctx.ext.free(page);
            let res = ctx.process_alloc_func_result(res)?;

            match &res {
                Ok(()) => {
                    log::trace!("Free {page:?}");
                }
                Err(err) => {
                    log::trace!("Free failed: {err}");
                }
            };

            Ok(ReturnValue::Value(Value::I32(res.is_err() as i32)))
        })
    }

    /// Infallible `gr_block_height` syscall.
    pub fn block_height(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let height_ptr = args.iter().read();

        syscall_trace!("block_height", height_ptr);

        ctx.run(RuntimeCosts::BlockHeight, |ctx| {
            let height = ctx.ext.block_height()?;

            let write_height = ctx.register_write_as(height_ptr);
            ctx.write_as(write_height, height.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_block_timestamp` syscall.
    pub fn block_timestamp(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let timestamp_ptr = args.iter().read();

        syscall_trace!("block_timestamp", timestamp_ptr);

        ctx.run(RuntimeCosts::BlockTimestamp, |ctx| {
            let timestamp = ctx.ext.block_timestamp()?;

            let write_timestamp = ctx.register_write_as(timestamp_ptr);
            ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_origin` syscall.
    pub fn origin(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let origin_ptr = args.iter().read();

        syscall_trace!("origin", origin_ptr);

        ctx.run(RuntimeCosts::Origin, |ctx| {
            let origin = ctx.ext.origin()?;

            let write_origin = ctx.register_write_as(origin_ptr);
            ctx.write_as(write_origin, origin.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_random` syscall.
    pub fn random(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (subject_ptr, bn_random_ptr) = args.iter().read_2();

        syscall_trace!("random", subject_ptr, bn_random_ptr);

        ctx.run(RuntimeCosts::Random, |ctx| {
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
        let (payload_ptr, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!("reply", payload_ptr, len, value_ptr, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Reply(len), |ctx| {
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
        let (payload_ptr, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6();

        syscall_trace!(
            "reply_wgas",
            payload_ptr,
            len,
            gas_limit,
            value_ptr,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Reply(len), |ctx| {
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
        let (value_ptr, delay, err_mid_ptr) = args.iter().read_3();

        syscall_trace!("reply_commit", value_ptr, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyCommit(0), |ctx| {
            let value = Self::register_and_read_value(ctx, value_ptr)?;

            ctx.ext
                .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_commit_wgas` syscall.
    pub fn reply_commit_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_4();

        syscall_trace!(
            "reply_commit_wgas",
            gas_limit,
            value_ptr,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyCommit(0), |ctx| {
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
        let (rid_value_ptr, payload_ptr, len, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!(
            "reservation_reply",
            rid_value_ptr,
            payload_ptr,
            len,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(
            err_mid_ptr,
            RuntimeCosts::ReservationReply(len),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_reservation_reply_commit` syscall.
    pub fn reservation_reply_commit(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (rid_value_ptr, delay, err_mid_ptr) = args.iter().read_3();

        syscall_trace!(
            "reservation_reply_commit",
            rid_value_ptr,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(
            err_mid_ptr,
            RuntimeCosts::ReservationReplyCommit(0),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_reply_to` syscall.
    pub fn reply_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let err_mid_ptr = args.iter().read();

        syscall_trace!("reply_to", err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyTo, |ctx| {
            ctx.ext.reply_to().map_err(Into::into)
        })
    }

    /// Fallible `gr_signal_from` syscall.
    pub fn signal_from(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let err_mid_ptr = args.iter().read();

        syscall_trace!("signal_from", err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SignalFrom, |ctx| {
            ctx.ext.signal_from().map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_push` syscall.
    pub fn reply_push(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (payload_ptr, len, err_len_ptr) = args.iter().read_3();

        syscall_trace!("reply_push", payload_ptr, len, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::ReplyPush(len), |ctx| {
            let read_payload = ctx.register_read(payload_ptr, len);
            let payload = ctx.read(read_payload)?;

            ctx.ext.reply_push(&payload).map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_input` syscall.
    pub fn reply_input(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (offset, len, value_ptr, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!("reply_input", offset, len, value_ptr, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyInput, |ctx| {
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
        let (offset, len, err_len_ptr) = args.iter().read_3();

        syscall_trace!("reply_push_input", offset, len, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::ReplyPushInput, |ctx| {
            ctx.ext.reply_push_input(offset, len).map_err(Into::into)
        })
    }

    /// Fallible `gr_reply_input_wgas` syscall.
    pub fn reply_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (offset, len, gas_limit, value_ptr, delay, err_mid_ptr) = args.iter().read_6();

        syscall_trace!(
            "reply_input_wgas",
            offset,
            len,
            gas_limit,
            value_ptr,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyInput, |ctx| {
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
        let (pid_value_ptr, offset, len, delay, err_mid_ptr) = args.iter().read_5();

        syscall_trace!("send_input", pid_value_ptr, offset, len, delay, err_mid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendInput, |ctx| {
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
        let (handle, offset, len, err_len_ptr) = args.iter().read_4();

        syscall_trace!("send_push_input", handle, offset, len, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::SendPushInput, |ctx| {
            ctx.ext
                .send_push_input(handle, offset, len)
                .map_err(Into::into)
        })
    }

    /// Fallible `gr_send_push_input_wgas` syscall.
    pub fn send_input_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (pid_value_ptr, offset, len, gas_limit, delay, err_mid_ptr) = args.iter().read_6();

        syscall_trace!(
            "send_input_wgas",
            pid_value_ptr,
            offset,
            len,
            gas_limit,
            delay,
            err_mid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendInput, |ctx| {
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
        let (data_ptr, data_len): (_, u32) = args.iter().read_2();

        syscall_trace!("debug", data_ptr, data_len);

        ctx.run(RuntimeCosts::Debug(data_len), |ctx| {
            let read_data = ctx.register_read(data_ptr, data_len);
            let data: RuntimeBuffer = ctx.read(read_data)?.try_into()?;

            let s = String::from_utf8(data.into_vec())?;
            ctx.ext.debug(&s)?;

            Ok(())
        })
    }

    /// Infallible `gr_panic` syscall.
    pub fn panic(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (data_ptr, data_len): (_, u32) = args.iter().read_2();

        syscall_trace!("panic", data_ptr, data_len);

        ctx.run(RuntimeCosts::Null, |ctx| {
            let read_data = ctx.register_read(data_ptr, data_len);
            let data = ctx.read(read_data).unwrap_or_default();

            let s = String::from_utf8_lossy(&data).to_string();

            Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
        })
    }

    /// Infallible `gr_oom_panic` syscall.
    pub fn oom_panic(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        syscall_trace!("oom panic");

        ctx.run(RuntimeCosts::Null, |_ctx| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
        })
    }

    /// Fallible `gr_reserve_gas` syscall.
    pub fn reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (gas, duration, err_rid_ptr) = args.iter().read_3();

        syscall_trace!("reserve_gas", gas, duration, err_rid_ptr);

        ctx.run_fallible::<_, _, LengthWithHash>(err_rid_ptr, RuntimeCosts::ReserveGas, |ctx| {
            ctx.ext.reserve_gas(gas, duration).map_err(Into::into)
        })
    }

    /// Fallible `gr_unreserve_gas` syscall.
    pub fn unreserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (reservation_id_ptr, err_unreserved_ptr) = args.iter().read_2();

        syscall_trace!("unreserve_gas", reservation_id_ptr, err_unreserved_ptr);

        ctx.run_fallible::<_, _, LengthWithGas>(
            err_unreserved_ptr,
            RuntimeCosts::UnreserveGas,
            |ctx| {
                let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
                let reservation_id = ctx.read_decoded(read_reservation_id)?;

                ctx.ext.unreserve_gas(reservation_id).map_err(Into::into)
            },
        )
    }

    /// Fallible `gr_system_reserve_gas` syscall.
    pub fn system_reserve_gas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (gas, err_len_ptr) = args.iter().read_2();

        syscall_trace!("system_reserve_gas", gas, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::SystemReserveGas, |ctx| {
            ctx.ext.system_reserve_gas(gas).map_err(Into::into)
        })
    }

    /// Infallible `gr_gas_available` syscall.
    pub fn gas_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let gas_ptr = args.iter().read();

        syscall_trace!("gas_available", gas_ptr);

        ctx.run(RuntimeCosts::GasAvailable, |ctx| {
            let gas = ctx.ext.gas_available()?;

            let write_gas = ctx.register_write_as(gas_ptr);
            ctx.write_as(write_gas, gas.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_message_id` syscall.
    pub fn message_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let message_id_ptr = args.iter().read();

        syscall_trace!("message_id", message_id_ptr);

        ctx.run(RuntimeCosts::MsgId, |ctx| {
            let message_id = ctx.ext.message_id()?;

            let write_message_id = ctx.register_write_as(message_id_ptr);
            ctx.write_as(write_message_id, message_id.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_program_id` syscall.
    pub fn program_id(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let program_id_ptr = args.iter().read();

        syscall_trace!("program_id", program_id_ptr);

        ctx.run(RuntimeCosts::ProgramId, |ctx| {
            let program_id = ctx.ext.program_id()?;

            let write_program_id = ctx.register_write_as(program_id_ptr);
            ctx.write_as(write_program_id, program_id.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_source` syscall.
    pub fn source(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let source_ptr = args.iter().read();

        syscall_trace!("source", source_ptr);

        ctx.run(RuntimeCosts::Source, |ctx| {
            let source = ctx.ext.source()?;

            let write_source = ctx.register_write_as(source_ptr);
            ctx.write_as(write_source, source.into_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_value` syscall.
    pub fn value(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let value_ptr = args.iter().read();

        syscall_trace!("value", value_ptr);

        ctx.run(RuntimeCosts::Value, |ctx| {
            let value = ctx.ext.value()?;

            let write_value = ctx.register_write_as(value_ptr);
            ctx.write_as(write_value, value.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_value_available` syscall.
    pub fn value_available(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let value_ptr = args.iter().read();

        syscall_trace!("value_available", value_ptr);

        ctx.run(RuntimeCosts::ValueAvailable, |ctx| {
            let value_available = ctx.ext.value_available()?;

            let write_value = ctx.register_write_as(value_ptr);
            ctx.write_as(write_value, value_available.to_le_bytes())
                .map_err(Into::into)
        })
    }

    /// Infallible `gr_leave` syscall.
    pub fn leave(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        syscall_trace!("leave");

        ctx.run(RuntimeCosts::Leave, |_ctx| {
            Err(ActorTerminationReason::Leave.into())
        })
    }

    /// Infallible `gr_wait` syscall.
    pub fn wait(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        syscall_trace!("wait");

        ctx.run(RuntimeCosts::Wait, |ctx| -> Result<(), _> {
            ctx.ext.wait()?;
            Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
        })
    }

    /// Infallible `gr_wait_for` syscall.
    pub fn wait_for(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let duration = args.iter().read();

        syscall_trace!("wait_for", duration);

        ctx.run(RuntimeCosts::WaitFor, |ctx| -> Result<(), _> {
            ctx.ext.wait_for(duration)?;
            Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
        })
    }

    /// Infallible `gr_wait_up_to` syscall.
    pub fn wait_up_to(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let duration = args.iter().read();

        syscall_trace!("wait_up_to", duration);

        ctx.run(RuntimeCosts::WaitUpTo, |ctx| -> Result<(), _> {
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
        let (message_id_ptr, delay, err_len_ptr) = args.iter().read_3();

        syscall_trace!("wake", message_id_ptr, delay, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::Wake, |ctx| {
            let read_message_id = ctx.register_read_decoded(message_id_ptr);
            let message_id = ctx.read_decoded(read_message_id)?;

            ctx.ext.wake(message_id, delay).map_err(Into::into)
        })
    }

    /// Fallible `gr_create_program` syscall.
    pub fn create_program(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (cid_value_ptr, salt_ptr, salt_len, payload_ptr, payload_len, delay, err_mid_pid_ptr) =
            args.iter().read_7();

        syscall_trace!(
            "create_program",
            cid_value_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            delay,
            err_mid_pid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithTwoHashes>(
            err_mid_pid_ptr,
            RuntimeCosts::CreateProgram(payload_len, payload_ptr),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_create_program_wgas` syscall.
    pub fn create_program_wgas(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        let (
            cid_value_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            gas_limit,
            delay,
            err_mid_pid_ptr,
        ) = args.iter().read_8();

        syscall_trace!(
            "create_program_wgas",
            cid_value_ptr,
            salt_ptr,
            salt_len,
            payload_ptr,
            payload_len,
            gas_limit,
            delay,
            err_mid_pid_ptr
        );

        ctx.run_fallible::<_, _, LengthWithTwoHashes>(
            err_mid_pid_ptr,
            RuntimeCosts::CreateProgram(payload_len, salt_len),
            |ctx| {
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
            },
        )
    }

    /// Fallible `gr_error` syscall.
    pub fn error(ctx: &mut Runtime<E>, args: &[Value]) -> SyscallOutput {
        // `error_bytes_ptr` is ptr for buffer of an error
        // `err_len_ptr` is ptr for len of the error occurred during this syscall
        let (error_bytes_ptr, err_len_ptr) = args.iter().read_2();

        syscall_trace!("error", error_bytes_ptr, err_len_ptr);

        ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::Error, |ctx| {
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
        syscall_trace!("forbidden");

        ctx.run(RuntimeCosts::Null, |_| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into())
        })
    }

    /// Infallible `gr_out_of_gas` syscall.
    pub fn out_of_gas(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        syscall_trace!("out_of_gas");

        ctx.set_termination_reason(
            ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded).into(),
        );

        Err(HostError)
    }

    /// Infallible `gr_out_of_allowance` syscall.
    pub fn out_of_allowance(ctx: &mut Runtime<E>, _args: &[Value]) -> SyscallOutput {
        syscall_trace!("out_of_allowance");

        ctx.set_termination_reason(ActorTerminationReason::GasAllowanceExceeded.into());

        Err(HostError)
    }
}

#[allow(clippy::type_complexity)]
trait WasmCompatibleIterator {
    fn read<T: WasmCompatible>(&mut self) -> T;

    fn read_2<T1: WasmCompatible, T2: WasmCompatible>(&mut self) -> (T1, T2) {
        (self.read(), self.read())
    }

    fn read_3<T1: WasmCompatible, T2: WasmCompatible, T3: WasmCompatible>(
        &mut self,
    ) -> (T1, T2, T3) {
        (self.read(), self.read(), self.read())
    }

    fn read_4<T1: WasmCompatible, T2: WasmCompatible, T3: WasmCompatible, T4: WasmCompatible>(
        &mut self,
    ) -> (T1, T2, T3, T4) {
        (self.read(), self.read(), self.read(), self.read())
    }

    fn read_5<
        T1: WasmCompatible,
        T2: WasmCompatible,
        T3: WasmCompatible,
        T4: WasmCompatible,
        T5: WasmCompatible,
    >(
        &mut self,
    ) -> (T1, T2, T3, T4, T5) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
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
    ) -> (T1, T2, T3, T4, T5, T6) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
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
    ) -> (T1, T2, T3, T4, T5, T6, T7) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
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
    ) -> (T1, T2, T3, T4, T5, T6, T7, T8) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
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
    ) -> (T1, T2, T3, T4, T5, T6, T7, T8, T9) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
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
    ) -> (T1, T2, T3, T4, T5, T6, T7, T8, T9, T10) {
        (
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
            self.read(),
        )
    }
}

impl<'a, I: Iterator<Item = &'a Value> + 'a> WasmCompatibleIterator for I {
    fn read<T: WasmCompatible>(&mut self) -> T {
        T::from(
            *self
                .next()
                .unwrap_or_else(|| unreachable!("Unable to get iterator next value")),
        )
    }
}

trait WasmCompatible: Sized {
    fn from(arg: Value) -> Self;

    fn throw_back(self) -> ReturnValue;
}

impl WasmCompatible for () {
    fn from(_arg: Value) -> Self {}

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Unit
    }
}

impl WasmCompatible for i32 {
    fn from(arg: Value) -> Self {
        if let Value::I32(val) = arg {
            val
        } else {
            unreachable!("Expected I32 value type")
        }
    }

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Value(Value::I32(self))
    }
}

impl WasmCompatible for u32 {
    fn from(arg: Value) -> Self {
        <i32 as WasmCompatible>::from(arg) as u32
    }

    fn throw_back(self) -> ReturnValue {
        (self as i32).throw_back()
    }
}

impl WasmCompatible for i64 {
    fn from(arg: Value) -> Self {
        if let Value::I64(val) = arg {
            val
        } else {
            unreachable!("Expected I64 value type")
        }
    }

    fn throw_back(self) -> ReturnValue {
        ReturnValue::Value(Value::I64(self))
    }
}

impl WasmCompatible for u64 {
    fn from(arg: Value) -> Self {
        <i64 as WasmCompatible>::from(arg) as u64
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
