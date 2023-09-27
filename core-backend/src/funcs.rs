// This file is part of Gear.

// Copyright (C) 2023 Gear Technologies Inc.
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

//! Syscall implementations generic over wasmi and sandbox backends.

use crate::{
    error::{
        ActorTerminationReason, BackendAllocSyscallError, BackendSyscallError, RunFallibleError,
        TrapExplanation, UndefinedTerminationReason, UnrecoverableExecutionError,
        UnrecoverableMemoryError,
    },
    memory::{MemoryAccessError, WasmMemoryRead},
    runtime::CallerWrap,
    BackendExternalities,
};
use alloc::string::{String, ToString};
use blake2_rfc::blake2b::blake2b;
use core::marker::PhantomData;
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    costs::RuntimeCosts,
    env::{DropPayloadLockBound, Externalities},
    gas::CounterType,
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
    pages::{PageNumber, PageU32Size, WasmPage},
};
use gear_core_backend_codegen::host;
use gear_core_errors::{MessageError, ReplyCode, SignalCode};
use gear_sandbox_env::HostError;
use gsys::{
    BlockNumberWithHash, ErrorBytes, ErrorWithBlockNumberAndValue, ErrorWithGas, ErrorWithHandle,
    ErrorWithHash, ErrorWithReplyCode, ErrorWithSignalCode, ErrorWithTwoHashes, Hash,
    HashWithValue, TwoHashesWithValue,
};

#[macro_export(local_inner_macros)]
macro_rules! syscall_args_trace {
    ($val:expr) => {
        {
            let s = ::core::stringify!($val);
            if s.ends_with("_ptr") {
                alloc::format!(", {} = {:#x?}", s, $val)
            } else {
                alloc::format!(", {} = {:?}", s, $val)
            }
        }
    };
    ($val:expr, $($rest:expr),+) => {
        {
            let mut s = syscall_args_trace!($val);
            s.push_str(&syscall_args_trace!($($rest),+));
            s
        }
    };
}

macro_rules! syscall_trace {
    ($name:expr, $($args:expr),+) => {
        {
            ::log::trace!(target: "syscalls", "{}{}", $name, syscall_args_trace!($($args),+));
        }
    };
    ($name:expr) => {
        {
            ::log::trace!(target: "syscalls", "{}", $name);
        }
    }
}

const PTR_SPECIAL: u32 = u32::MAX;

pub(crate) struct FuncsHandler<Ext: Externalities + 'static> {
    _phantom: PhantomData<Ext>,
}

impl<Ext> FuncsHandler<Ext>
where
    Ext: BackendExternalities + 'static,
    Ext::UnrecoverableError: BackendSyscallError,
    RunFallibleError: From<Ext::FallibleError>,
    Ext::AllocError: BackendAllocSyscallError<ExtError = Ext::UnrecoverableError>,
{
    /// !!! Usage warning: make sure to do it before any other read/write,
    /// because it may contain registered read.
    fn register_and_read_value(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        value_ptr: u32,
    ) -> Result<u128, MemoryAccessError> {
        if value_ptr != PTR_SPECIAL {
            let read_value = ctx.manager.register_read_decoded(value_ptr);
            return ctx.read_decoded(read_value);
        }

        Ok(0)
    }

    fn read_message_payload(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        read_payload: WasmMemoryRead,
    ) -> Result<Payload, RunFallibleError> {
        ctx.read(read_payload)?
            .try_into()
            .map_err(|PayloadSizeError| MessageError::MaxMessageSizeExceed.into())
            .map_err(RunFallibleError::FallibleExt)
    }

    // TODO #3037
    #[allow(clippy::too_many_arguments)]
    #[host(fallible, wgas, cost = RuntimeCosts::Send(len))]
    pub fn send(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_hash_val = ctx.manager.register_read_as(pid_value_ptr);
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_hash_val)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .send(HandlePacket::new(destination.into(), payload, value), delay)
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::SendCommit)]
    pub fn send_commit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        handle: u32,
        pid_value_ptr: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_pid_value = ctx.manager.register_read_as(pid_value_ptr);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        ctx.ext_mut()
            .send_commit(
                handle,
                HandlePacket::new(destination.into(), Default::default(), value),
                delay,
            )
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SendInit, err = ErrorWithHandle)]
    pub fn send_init(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        ctx.ext_mut().send_init().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SendPush(len), err = ErrorBytes)]
    pub fn send_push(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        handle: u32,
        payload_ptr: u32,
        len: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        ctx.ext_mut()
            .send_push(handle, &payload)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationSend(len))]
    pub fn reservation_send(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        rid_pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_rid_pid_value = ctx.manager.register_read_as(rid_pid_value_ptr);
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let TwoHashesWithValue {
            hash1: reservation_id,
            hash2: destination,
            value,
        } = ctx.read_as(read_rid_pid_value)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .reservation_send(
                reservation_id.into(),
                HandlePacket::new(destination.into(), payload, value),
                delay,
            )
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationSendCommit)]
    pub fn reservation_send_commit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        handle: u32,
        rid_pid_value_ptr: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_rid_pid_value = ctx.manager.register_read_as(rid_pid_value_ptr);
        let TwoHashesWithValue {
            hash1: reservation_id,
            hash2: destination,
            value,
        } = ctx.read_as(read_rid_pid_value)?;

        ctx.ext_mut()
            .reservation_send_commit(
                reservation_id.into(),
                handle,
                HandlePacket::new(destination.into(), Default::default(), value),
                delay,
            )
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::Read, err = ErrorBytes)]
    pub fn read(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        at: u32,
        len: u32,
        buffer_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let payload_lock = ctx.ext_mut().lock_payload(at, len)?;
        payload_lock
            .drop_with::<MemoryAccessError, _>(|payload_access| {
                let write_buffer = ctx.manager.register_write(buffer_ptr, len);
                let write_res = ctx.write(write_buffer, payload_access.as_slice());
                let unlock_bound = ctx.ext_mut().unlock_payload(payload_access.into_lock());

                DropPayloadLockBound::from((unlock_bound, write_res))
            })
            .into_inner()
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Size)]
    pub fn size(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        size_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let size = ctx.ext_mut().size()? as u32;

        let write_size = ctx.manager.register_write_as(size_ptr);
        ctx.write_as(write_size, size.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Exit)]
    pub fn exit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        inheritor_id_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_inheritor_id = ctx.manager.register_read_decoded(inheritor_id_ptr);
        let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
        Err(ActorTerminationReason::Exit(inheritor_id).into())
    }

    #[host(fallible, cost = RuntimeCosts::ReplyCode, err = ErrorWithReplyCode)]
    pub fn reply_code(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .reply_code()
            .map(ReplyCode::to_bytes)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SignalCode, err = ErrorWithSignalCode)]
    pub fn signal_code(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .signal_code()
            .map(SignalCode::to_u32)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Alloc(pages))]
    pub fn alloc(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        pages: u32,
    ) -> Result<(u64, u32), HostError> {
        let res = ctx.alloc(pages);
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
        Ok(page)
    }

    #[host(cost = RuntimeCosts::Free)]
    pub fn free(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        page_no: u32,
    ) -> Result<(u64, i32), HostError> {
        let page = WasmPage::new(page_no).map_err(|_| {
            UndefinedTerminationReason::Actor(ActorTerminationReason::Trap(
                TrapExplanation::Unknown,
            ))
        })?;

        let res = ctx.ext_mut().free(page);
        let res = ctx.process_alloc_func_result(res)?;

        match &res {
            Ok(()) => {
                log::trace!("Free {page:?}");
            }
            Err(err) => {
                log::trace!("Free failed: {err}");
            }
        };

        Ok(res.is_err() as i32)
    }

    #[host(cost = RuntimeCosts::BlockHeight)]
    pub fn block_height(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        height_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let height = ctx.ext_mut().block_height()?;

        let write_height = ctx.manager.register_write_as(height_ptr);
        ctx.write_as(write_height, height.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::BlockTimestamp)]
    pub fn block_timestamp(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        timestamp_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let timestamp = ctx.ext_mut().block_timestamp()?;

        let write_timestamp = ctx.manager.register_write_as(timestamp_ptr);
        ctx.write_as(write_timestamp, timestamp.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Random)]
    pub fn random(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        subject_ptr: u32,
        bn_random_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_subject = ctx.manager.register_read_decoded(subject_ptr);
        let write_bn_random = ctx.manager.register_write_as(bn_random_ptr);

        let raw_subject: Hash = ctx.read_decoded(read_subject)?;

        let (random, bn) = ctx.ext_mut().random()?;
        let subject = [&raw_subject, random].concat();

        let mut hash = [0; 32];
        hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

        ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::Reply(len))]
    pub fn reply(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        payload_ptr: u32,
        len: u32,
        value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let value = Self::register_and_read_value(ctx, value_ptr)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .reply(ReplyPacket::new(payload, value))
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyCommit)]
    pub fn reply_commit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let value = Self::register_and_read_value(ctx, value_ptr)?;

        ctx.ext_mut()
            .reply_commit(ReplyPacket::new(Default::default(), value))
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReply(len))]
    pub fn reservation_reply(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        rid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_rid_value = ctx.manager.register_read_as(rid_value_ptr);
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let HashWithValue {
            hash: reservation_id,
            value,
        } = ctx.read_as(read_rid_value)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        ctx.ext_mut()
            .reservation_reply(reservation_id.into(), ReplyPacket::new(payload, value))
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReplyCommit)]
    pub fn reservation_reply_commit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        rid_value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_rid_value = ctx.manager.register_read_as(rid_value_ptr);
        let HashWithValue {
            hash: reservation_id,
            value,
        } = ctx.read_as(read_rid_value)?;

        ctx.ext_mut()
            .reservation_reply_commit(
                reservation_id.into(),
                ReplyPacket::new(Default::default(), value),
            )
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyTo)]
    pub fn reply_to(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        ctx.ext_mut().reply_to().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SignalFrom)]
    pub fn signal_from(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut().signal_from().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyPush(len), err = ErrorBytes)]
    pub fn reply_push(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        payload_ptr: u32,
        len: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_payload = ctx.manager.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        ctx.ext_mut().reply_push(&payload).map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyInput)]
    pub fn reply_input(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        offset: u32,
        len: u32,
        value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        // Charge for `len` is inside `reply_push_input`
        let value = Self::register_and_read_value(ctx, value_ptr)?;

        let mut f = || {
            ctx.ext_mut().reply_push_input(offset, len)?;
            ctx.ext_mut()
                .reply_commit(ReplyPacket::new(Default::default(), value))
        };

        f().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyPushInput, err = ErrorBytes)]
    pub fn reply_push_input(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        offset: u32,
        len: u32,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .reply_push_input(offset, len)
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    #[host(fallible, wgas, cost = RuntimeCosts::SendInput)]
    pub fn send_input(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        pid_value_ptr: u32,
        offset: u32,
        len: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        // Charge for `len` inside `send_push_input`
        let read_pid_value = ctx.manager.register_read_as(pid_value_ptr);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        let mut f = || {
            let handle = ctx.ext_mut().send_init()?;
            ctx.ext_mut().send_push_input(handle, offset, len)?;
            ctx.ext_mut().send_commit(
                handle,
                HandlePacket::new(destination.into(), Default::default(), value),
                delay,
            )
        };

        f().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SendPushInput, err = ErrorBytes)]
    pub fn send_push_input(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        handle: u32,
        offset: u32,
        len: u32,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .send_push_input(handle, offset, len)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Debug(data_len))]
    pub fn debug(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        data_ptr: u32,
        data_len: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_data = ctx.manager.register_read(data_ptr, data_len);
        let data: RuntimeBuffer = ctx
            .read(read_data)?
            .try_into()
            .map_err(|RuntimeBufferSizeError| {
                UnrecoverableMemoryError::RuntimeAllocOutOfBounds.into()
            })
            .map_err(TrapExplanation::UnrecoverableExt)?;

        let s = String::from_utf8(data.into_vec())
            .map_err(|_err| UnrecoverableExecutionError::InvalidDebugString.into())
            .map_err(TrapExplanation::UnrecoverableExt)?;
        ctx.ext_mut().debug(&s)?;

        Ok(())
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn panic(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        data_ptr: u32,
        data_len: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_data = ctx.manager.register_read(data_ptr, data_len);
        let data = ctx.read(read_data).unwrap_or_default();

        let s = String::from_utf8_lossy(&data).to_string();

        Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn oom_panic(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
    }

    #[host(fallible, cost = RuntimeCosts::ReserveGas)]
    pub fn reserve_gas(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        gas_value: u64,
        duration: u32,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .reserve_gas(gas_value, duration)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyDeposit, err = ErrorBytes)]
    pub fn reply_deposit(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        message_id_ptr: u32,
        gas_value: u64,
    ) -> Result<(u64, ()), HostError> {
        let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
        let message_id = ctx.read_decoded(read_message_id)?;

        ctx.ext_mut()
            .reply_deposit(message_id, gas_value)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::UnreserveGas, err = ErrorWithGas)]
    pub fn unreserve_gas(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        reservation_id_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_reservation_id = ctx.manager.register_read_decoded(reservation_id_ptr);
        let reservation_id = ctx.read_decoded(read_reservation_id)?;

        ctx.ext_mut()
            .unreserve_gas(reservation_id)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SystemReserveGas, err = ErrorBytes)]
    pub fn system_reserve_gas(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        gas_value: u64,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut()
            .system_reserve_gas(gas_value)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::GasAvailable)]
    pub fn gas_available(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        gas_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let gas_available = ctx.ext_mut().gas_available()?;

        let write_gas = ctx.manager.register_write_as(gas_ptr);
        ctx.write_as(write_gas, gas_available.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::MsgId)]
    pub fn message_id(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        message_id_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let message_id = ctx.ext_mut().message_id()?;

        let write_message_id = ctx.manager.register_write_as(message_id_ptr);
        ctx.write_as(write_message_id, message_id.into_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::ProgramId)]
    pub fn program_id(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        program_id_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let program_id = ctx.ext_mut().program_id()?;

        let write_program_id = ctx.manager.register_write_as(program_id_ptr);
        ctx.write_as(write_program_id, program_id.into_bytes())
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::PayProgramRent, err = ErrorWithBlockNumberAndValue)]
    pub fn pay_program_rent(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        rent_pid_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_rent_pid = ctx.manager.register_read_as(rent_pid_ptr);

        let HashWithValue {
            hash: program_id,
            value: rent,
        } = ctx.read_as(read_rent_pid)?;

        ctx.ext_mut()
            .pay_program_rent(program_id.into(), rent)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Source)]
    pub fn source(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        source_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let source = ctx.ext_mut().source()?;

        let write_source = ctx.manager.register_write_as(source_ptr);
        ctx.write_as(write_source, source.into_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Value)]
    pub fn value(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let value = ctx.ext_mut().value()?;

        let write_value = ctx.manager.register_write_as(value_ptr);
        ctx.write_as(write_value, value.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::ValueAvailable)]
    pub fn value_available(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        value_ptr: u32,
    ) -> Result<(u64, ()), HostError> {
        let value_available = ctx.ext_mut().value_available()?;

        let write_value = ctx.manager.register_write_as(value_ptr);
        ctx.write_as(write_value, value_available.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Leave)]
    pub fn leave(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        Err(ActorTerminationReason::Leave.into())
    }

    #[host(cost = RuntimeCosts::Wait)]
    pub fn wait(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        ctx.ext_mut().wait()?;
        Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
    }

    #[host(cost = RuntimeCosts::WaitFor)]
    pub fn wait_for(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        duration: u32,
    ) -> Result<(u64, ()), HostError> {
        ctx.ext_mut().wait_for(duration)?;
        Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
    }

    #[host(cost = RuntimeCosts::WaitUpTo)]
    pub fn wait_up_to(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        duration: u32,
    ) -> Result<(u64, ()), HostError> {
        let waited_type = if ctx.ext_mut().wait_up_to(duration)? {
            MessageWaitedType::WaitUpToFull
        } else {
            MessageWaitedType::WaitUpTo
        };
        Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
    }

    #[host(fallible, cost = RuntimeCosts::Wake, err = ErrorBytes)]
    pub fn wake(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        message_id_ptr: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_message_id = ctx.manager.register_read_decoded(message_id_ptr);
        let message_id = ctx.read_decoded(read_message_id)?;

        ctx.ext_mut().wake(message_id, delay).map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    #[host(fallible, wgas, cost = RuntimeCosts::CreateProgram(payload_len, salt_len), err = ErrorWithTwoHashes)]
    pub fn create_program(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        gas: u64,
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        delay: u32,
    ) -> Result<(u64, ()), HostError> {
        let read_cid_value = ctx.manager.register_read_as(cid_value_ptr);
        let read_salt = ctx.manager.register_read(salt_ptr, salt_len);
        let read_payload = ctx.manager.register_read(payload_ptr, payload_len);
        let HashWithValue {
            hash: code_id,
            value,
        } = ctx.read_as(read_cid_value)?;
        let salt = Self::read_message_payload(ctx, read_salt)?;
        let payload = Self::read_message_payload(ctx, read_payload)?;

        let message_id = ctx.ext_mut().message_id()?;

        ctx.ext_mut()
            .create_program(
                InitPacket::new(code_id.into(), salt, payload, Some(message_id), value),
                delay,
            )
            .map_err(Into::into)
    }

    pub fn forbidden(ctx: &mut CallerWrap<'_, '_, Ext>, gas: u64) -> Result<(u64, ()), HostError> {
        syscall_trace!("forbidden");

        ctx.run_any(gas, RuntimeCosts::Null, |_| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into())
        })
    }

    pub fn out_of_gas(
        ctx: &mut CallerWrap<'_, '_, Ext>,
        _gas: u64,
    ) -> Result<(u64, ()), HostError> {
        syscall_trace!("out_of_gas");

        let ext = ctx.ext_mut();
        let current_counter = ext.current_counter_type();
        log::trace!(target: "syscalls", "[out_of_gas] Current counter in global represents {current_counter:?}");

        if current_counter == CounterType::GasAllowance {
            // We manually decrease it to 0 because global won't be affected
            // since it didn't pass comparison to argument of `gas_charge()`
            ext.decrease_current_counter_to(0);
        }

        let termination_reason: ActorTerminationReason = current_counter.into();

        ctx.set_termination_reason(termination_reason.into());
        Err(HostError)
    }
}
