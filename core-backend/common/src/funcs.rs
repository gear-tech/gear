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
    memory::MemoryAccessError, runtime::Runtime, syscall_trace, ActorTerminationReason,
    BackendAllocExternalitiesError, BackendExternalities, BackendExternalitiesError,
    MessageWaitedType, TerminationReason, TrapExplanation, PTR_SPECIAL,
};
use alloc::string::{String, ToString};
use blake2_rfc::blake2b::blake2b;
use core::marker::PhantomData;
use gear_backend_codegen::host;
use gear_core::{
    buffer::RuntimeBuffer,
    costs::RuntimeCosts,
    env::Externalities,
    memory::{PageU32Size, WasmPage},
    message::{HandlePacket, InitPacket, ReplyPacket},
};
use gsys::{
    BlockNumberWithHash, ErrorBytes, ErrorWithBlockNumberAndValue, ErrorWithGas, ErrorWithHandle,
    ErrorWithHash, ErrorWithStatus, ErrorWithTwoHashes, Hash, HashWithValue, TwoHashesWithValue,
};

pub struct FuncsHandler<Ext: Externalities + 'static, Runtime> {
    _phantom: PhantomData<(Ext, Runtime)>,
}

impl<Ext, R> FuncsHandler<Ext, R>
where
    Ext: BackendExternalities + 'static,
    Ext::Error: BackendExternalitiesError,
    Ext::AllocError: BackendAllocExternalitiesError<ExtError = Ext::Error>,
    R: Runtime<Ext>,
{
    /// !!! Usage warning: make sure to do it before any other read/write,
    /// because it may contain registered read.
    fn register_and_read_value(ctx: &mut R, value_ptr: u32) -> Result<u128, MemoryAccessError> {
        if value_ptr != PTR_SPECIAL {
            let read_value = ctx.register_read_decoded(value_ptr);
            return ctx.read_decoded(read_value);
        }

        Ok(0)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::Send(len))]
    pub fn send(
        ctx: &mut R,
        pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_hash_val = ctx.register_read_as(pid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);
        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_hash_val)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        ctx.ext_mut()
            .send(HandlePacket::new(destination.into(), payload, value), delay)
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::SendCommit)]
    pub fn send_commit(
        ctx: &mut R,
        handle: u32,
        pid_value_ptr: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_pid_value = ctx.register_read_as(pid_value_ptr);
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
    pub fn send_init(ctx: &mut R) -> Result<(), R::Error> {
        ctx.ext_mut().send_init().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SendPush(len), err = ErrorBytes)]
    pub fn send_push(ctx: &mut R, handle: u32, payload_ptr: u32, len: u32) -> Result<(), R::Error> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        ctx.ext_mut()
            .send_push(handle, &payload)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationSend(len))]
    pub fn reservation_send(
        ctx: &mut R,
        rid_pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);
        let TwoHashesWithValue {
            hash1: reservation_id,
            hash2: destination,
            value,
        } = ctx.read_as(read_rid_pid_value)?;
        let payload = ctx.read(read_payload)?.try_into()?;

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
        ctx: &mut R,
        handle: u32,
        rid_pid_value_ptr: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
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
    pub fn read(ctx: &mut R, at: u32, len: u32, buffer_ptr: u32) -> Result<(), R::Error> {
        let (buffer, mut gas_left) = ctx.ext_mut().read(at, len)?;
        let buffer = buffer.to_vec();

        let write_buffer = ctx.register_write(buffer_ptr, len);
        ctx.memory_manager_write(write_buffer, &buffer, &mut gas_left)?;

        ctx.ext_mut().set_gas_left(gas_left);
        Ok(())
    }

    #[host(cost = RuntimeCosts::Size)]
    pub fn size(ctx: &mut R, size_ptr: u32) -> Result<(), R::Error> {
        let size = ctx.ext_mut().size()? as u32;

        let write_size = ctx.register_write_as(size_ptr);
        ctx.write_as(write_size, size.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Exit)]
    pub fn exit(ctx: &mut R, inheritor_id_ptr: u32) -> Result<(), R::Error> {
        let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);
        let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
        Err(ActorTerminationReason::Exit(inheritor_id).into())
    }

    #[host(fallible, cost = RuntimeCosts::StatusCode, err = ErrorWithStatus)]
    pub fn status_code(ctx: &mut R) -> Result<(), R::Error> {
        ctx.ext_mut().status_code().map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Alloc)]
    pub fn alloc(ctx: &mut R, pages: u32) -> Result<u32, R::Error> {
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
    pub fn free(ctx: &mut R, page_no: u32) -> Result<i32, R::Error> {
        let page = WasmPage::new(page_no).map_err(|_| {
            TerminationReason::Actor(ActorTerminationReason::Trap(TrapExplanation::Unknown))
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
    pub fn block_height(ctx: &mut R, height_ptr: u32) -> Result<(), R::Error> {
        let height = ctx.ext_mut().block_height()?;

        let write_height = ctx.register_write_as(height_ptr);
        ctx.write_as(write_height, height.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::BlockTimestamp)]
    pub fn block_timestamp(ctx: &mut R, timestamp_ptr: u32) -> Result<(), R::Error> {
        let timestamp = ctx.ext_mut().block_timestamp()?;

        let write_timestamp = ctx.register_write_as(timestamp_ptr);
        ctx.write_as(write_timestamp, timestamp.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Random)]
    pub fn random(ctx: &mut R, subject_ptr: u32, bn_random_ptr: u32) -> Result<(), R::Error> {
        let read_subject = ctx.register_read_decoded(subject_ptr);
        let write_bn_random = ctx.register_write_as(bn_random_ptr);

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
        ctx: &mut R,
        payload_ptr: u32,
        len: u32,
        value_ptr: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let value = Self::register_and_read_value(ctx, value_ptr)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        ctx.ext_mut()
            .reply(ReplyPacket::new(payload, value))
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyCommit)]
    pub fn reply_commit(ctx: &mut R, value_ptr: u32, delay: u32) -> Result<(), R::Error> {
        let value = Self::register_and_read_value(ctx, value_ptr)?;

        ctx.ext_mut()
            .reply_commit(ReplyPacket::new(Default::default(), value))
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReply(len))]
    pub fn reservation_reply(
        ctx: &mut R,
        rid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_rid_value = ctx.register_read_as(rid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);
        let HashWithValue {
            hash: reservation_id,
            value,
        } = ctx.read_as(read_rid_value)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        ctx.ext_mut()
            .reservation_reply(reservation_id.into(), ReplyPacket::new(payload, value))
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReplyCommit)]
    pub fn reservation_reply_commit(
        ctx: &mut R,
        rid_value_ptr: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_rid_value = ctx.register_read_as(rid_value_ptr);
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
    pub fn reply_to(ctx: &mut R) -> Result<(), R::Error> {
        ctx.ext_mut().reply_to().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SignalFrom)]
    pub fn signal_from(ctx: &mut R) -> Result<(), R::Error> {
        ctx.ext_mut().signal_from().map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyPush(len), err = ErrorBytes)]
    pub fn reply_push(ctx: &mut R, payload_ptr: u32, len: u32) -> Result<(), R::Error> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        ctx.ext_mut().reply_push(&payload).map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyInput)]
    pub fn reply_input(
        ctx: &mut R,
        offset: u32,
        len: u32,
        value_ptr: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
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
    pub fn reply_push_input(ctx: &mut R, offset: u32, len: u32) -> Result<(), R::Error> {
        ctx.ext_mut()
            .reply_push_input(offset, len)
            .map_err(Into::into)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::SendInput)]
    pub fn send_input(
        ctx: &mut R,
        pid_value_ptr: u32,
        offset: u32,
        len: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        // Charge for `len` inside `send_push_input`
        let read_pid_value = ctx.register_read_as(pid_value_ptr);
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
        ctx: &mut R,
        handle: u32,
        offset: u32,
        len: u32,
    ) -> Result<(), R::Error> {
        ctx.ext_mut()
            .send_push_input(handle, offset, len)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Debug(data_len))]
    pub fn debug(ctx: &mut R, data_ptr: u32, data_len: u32) -> Result<(), R::Error> {
        let read_data = ctx.register_read(data_ptr, data_len);
        let data: RuntimeBuffer = ctx.read(read_data)?.try_into()?;

        let s = String::from_utf8(data.into_vec())?;
        ctx.ext_mut().debug(&s)?;

        Ok(())
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn panic(ctx: &mut R, data_ptr: u32, data_len: u32) -> Result<(), R::Error> {
        let read_data = ctx.register_read(data_ptr, data_len);
        let data = ctx.read(read_data).unwrap_or_default();

        let s = String::from_utf8_lossy(&data).to_string();

        Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn oom_panic(ctx: &mut R) -> Result<(), R::Error> {
        Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
    }

    #[host(fallible, cost = RuntimeCosts::ReserveGas)]
    pub fn reserve_gas(ctx: &mut R, gas: u64, duration: u32) -> Result<(), R::Error> {
        ctx.ext_mut().reserve_gas(gas, duration).map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::ReplyDeposit, err = ErrorBytes)]
    pub fn reply_deposit(ctx: &mut R, message_id_ptr: u32, gas: u64) -> Result<(), R::Error> {
        let read_message_id = ctx.register_read_decoded(message_id_ptr);
        let message_id = ctx.read_decoded(read_message_id)?;

        ctx.ext_mut()
            .reply_deposit(message_id, gas)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::UnreserveGas, err = ErrorWithGas)]
    pub fn unreserve_gas(ctx: &mut R, reservation_id_ptr: u32) -> Result<(), R::Error> {
        let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
        let reservation_id = ctx.read_decoded(read_reservation_id)?;

        ctx.ext_mut()
            .unreserve_gas(reservation_id)
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::SystemReserveGas, err = ErrorBytes)]
    pub fn system_reserve_gas(ctx: &mut R, gas: u64) -> Result<(), R::Error> {
        ctx.ext_mut().system_reserve_gas(gas).map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::GasAvailable)]
    pub fn gas_available(ctx: &mut R, gas_ptr: u32) -> Result<(), R::Error> {
        let gas = ctx.ext_mut().gas_available()?;

        let write_gas = ctx.register_write_as(gas_ptr);
        ctx.write_as(write_gas, gas.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::MsgId)]
    pub fn message_id(ctx: &mut R, message_id_ptr: u32) -> Result<(), R::Error> {
        let message_id = ctx.ext_mut().message_id()?;

        let write_message_id = ctx.register_write_as(message_id_ptr);
        ctx.write_as(write_message_id, message_id.into_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::ProgramId)]
    pub fn program_id(ctx: &mut R, program_id_ptr: u32) -> Result<(), R::Error> {
        let program_id = ctx.ext_mut().program_id()?;

        let write_program_id = ctx.register_write_as(program_id_ptr);
        ctx.write_as(write_program_id, program_id.into_bytes())
            .map_err(Into::into)
    }

    #[host(fallible, cost = RuntimeCosts::PayProgramRent, err = ErrorWithBlockNumberAndValue)]
    pub fn pay_program_rent(ctx: &mut R, rent_pid_ptr: u32) -> Result<(), R::Error> {
        let read_rent_pid = ctx.register_read_as(rent_pid_ptr);

        let HashWithValue {
            hash: program_id,
            value: rent,
        } = ctx.read_as(read_rent_pid)?;

        ctx.ext_mut()
            .pay_program_rent(program_id.into(), rent)
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Source)]
    pub fn source(ctx: &mut R, source_ptr: u32) -> Result<(), R::Error> {
        let source = ctx.ext_mut().source()?;

        let write_source = ctx.register_write_as(source_ptr);
        ctx.write_as(write_source, source.into_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Value)]
    pub fn value(ctx: &mut R, value_ptr: u32) -> Result<(), R::Error> {
        let value = ctx.ext_mut().value()?;

        let write_value = ctx.register_write_as(value_ptr);
        ctx.write_as(write_value, value.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::ValueAvailable)]
    pub fn value_available(ctx: &mut R, value_ptr: u32) -> Result<(), R::Error> {
        let value_available = ctx.ext_mut().value_available()?;

        let write_value = ctx.register_write_as(value_ptr);
        ctx.write_as(write_value, value_available.to_le_bytes())
            .map_err(Into::into)
    }

    #[host(cost = RuntimeCosts::Leave)]
    pub fn leave(ctx: &mut R) -> Result<(), R::Error> {
        Err(ActorTerminationReason::Leave.into())
    }

    #[host(cost = RuntimeCosts::Wait)]
    pub fn wait(ctx: &mut R) -> Result<(), R::Error> {
        ctx.ext_mut().wait()?;
        Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
    }

    #[host(cost = RuntimeCosts::WaitFor)]
    pub fn wait_for(ctx: &mut R, duration: u32) -> Result<(), R::Error> {
        ctx.ext_mut().wait_for(duration)?;
        Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
    }

    #[host(cost = RuntimeCosts::WaitUpTo)]
    pub fn wait_up_to(ctx: &mut R, duration: u32) -> Result<(), R::Error> {
        let waited_type = if ctx.ext_mut().wait_up_to(duration)? {
            MessageWaitedType::WaitUpToFull
        } else {
            MessageWaitedType::WaitUpTo
        };
        Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
    }

    #[host(fallible, cost = RuntimeCosts::Wake, err = ErrorBytes)]
    pub fn wake(ctx: &mut R, message_id_ptr: u32, delay: u32) -> Result<(), R::Error> {
        let read_message_id = ctx.register_read_decoded(message_id_ptr);
        let message_id = ctx.read_decoded(read_message_id)?;

        ctx.ext_mut().wake(message_id, delay).map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    #[host(fallible, wgas, cost = RuntimeCosts::CreateProgram(payload_len, salt_len), err = ErrorWithTwoHashes)]
    pub fn create_program(
        ctx: &mut R,
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        delay: u32,
    ) -> Result<(), R::Error> {
        let read_cid_value = ctx.register_read_as(cid_value_ptr);
        let read_salt = ctx.register_read(salt_ptr, salt_len);
        let read_payload = ctx.register_read(payload_ptr, payload_len);
        let HashWithValue {
            hash: code_id,
            value,
        } = ctx.read_as(read_cid_value)?;
        let salt = ctx.read(read_salt)?.try_into()?;
        let payload = ctx.read(read_payload)?.try_into()?;

        ctx.ext_mut()
            .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
            .map_err(Into::into)
    }

    pub fn forbidden(ctx: &mut R) -> Result<(), R::Error> {
        syscall_trace!("forbidden");

        ctx.run_any(RuntimeCosts::Null, |_| {
            Err(ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into())
        })
    }

    pub fn out_of_gas(ctx: &mut R) -> Result<(), R::Error> {
        syscall_trace!("out_of_gas");

        ctx.set_termination_reason(
            ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded).into(),
        );

        Err(R::unreachable_error())
    }

    pub fn out_of_allowance(ctx: &mut R) -> Result<(), R::Error> {
        syscall_trace!("out_of_allowance");

        ctx.set_termination_reason(ActorTerminationReason::GasAllowanceExceeded.into());

        Err(R::unreachable_error())
    }
}
