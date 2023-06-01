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

mod internal;

use crate::{funcs::internal::CallerWrap, memory::MemoryWrapRef, state::HostState};
use alloc::string::{String, ToString};
use blake2_rfc::blake2b::blake2b;
use codec::{Decode, Encode};
use core::{convert::TryInto, marker::PhantomData};
use gear_backend_codegen::host;
use gear_backend_common::{
    memory::{MemoryAccessError, MemoryAccessRecorder, MemoryOwner},
    syscall_trace, ActorTerminationReason, BackendAllocExtError, BackendExt, BackendExtError,
    BackendState, TerminationReason, TrapExplanation,
};
use gear_core::{
    buffer::RuntimeBuffer,
    costs::RuntimeCosts,
    env::Ext,
    memory::{PageU32Size, WasmPage},
    message::{HandlePacket, InitPacket, MessageWaitedType, ReplyPacket},
};
use gear_core_errors::ExtError;
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthBytes, LengthWithBlockNumberAndValue,
    LengthWithCode, LengthWithGas, LengthWithHandle, LengthWithHash, LengthWithTwoHashes,
    TwoHashesWithValue,
};
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
};

pub(crate) struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

type FnResult<T> = Result<(T,), Trap>;
type EmptyOutput = Result<(), Trap>;

impl<E> FuncsHandler<E>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
    E::AllocError: BackendAllocExtError<ExtError = E::Error>,
{
    #[host(fallible, wgas, cost = RuntimeCosts::Send(len))]
    pub fn send(
        ctx: CallerWrap<E>,
        pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_hash_val = ctx.register_read_as(pid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);

        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_hash_val)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        let state = ctx.host_state_mut();
        state
            .ext
            .send(HandlePacket::new(destination.into(), payload, value), delay)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::SendCommit)]
    pub fn send_commit(
        ctx: CallerWrap<E>,
        handle: u32,
        pid_value_ptr: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_pid_value = ctx.register_read_as(pid_value_ptr);

        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        let state = ctx.host_state_mut();
        state.ext.send_commit(
            handle,
            HandlePacket::new(destination.into(), Default::default(), value),
            delay,
        )
    }

    #[host(fallible, cost = RuntimeCosts::SendInit, err_len = LengthWithHandle)]
    pub fn send_init(ctx: CallerWrap<E>) -> Result<u32> {
        let state = ctx.host_state_mut();
        state.ext.send_init()
    }

    #[host(fallible, cost = RuntimeCosts::SendPush(len), err_len = LengthBytes)]
    pub fn send_push(ctx: CallerWrap<E>, handle: u32, payload_ptr: u32, len: u32) -> Result<()> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        let state = ctx.host_state_mut();
        state.ext.send_push(handle, &payload)
    }

    #[host(fallible, cost = RuntimeCosts::ReservationSend(len))]
    pub fn reservation_send(
        ctx: CallerWrap<E>,
        rid_pid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);

        let TwoHashesWithValue {
            hash1: reservation_id,
            hash2: destination,
            value,
        } = ctx.read_as(read_rid_pid_value)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        let state = ctx.host_state_mut();
        state.ext.reservation_send(
            reservation_id.into(),
            HandlePacket::new(destination.into(), payload, value),
            delay,
        )
    }

    #[host(fallible, cost = RuntimeCosts::ReservationSendCommit)]
    pub fn reservation_send_commit(
        ctx: CallerWrap<E>,
        handle: u32,
        rid_pid_value_ptr: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);

        let TwoHashesWithValue {
            hash1: reservation_id,
            hash2: destination,
            value,
        } = ctx.read_as(read_rid_pid_value)?;

        let state = ctx.host_state_mut();
        state.ext.reservation_send_commit(
            reservation_id.into(),
            handle,
            HandlePacket::new(destination.into(), Default::default(), value),
            delay,
        )
    }

    #[host(fallible_state_taken, cost = RuntimeCosts::Read, err_len = LengthBytes)]
    pub fn read(ctx: CallerWrap<E>, at: u32, len: u32, buffer_ptr: u32) -> Result<()> {
        // State is taken, so we cannot use `MemoryOwner` functions from `CallerWrap`.
        let (buffer, mut gas_left) = state.ext.read(at, len)?;

        let write_buffer = ctx.register_write(buffer_ptr, len);

        let mut memory = CallerWrap::memory(&mut ctx.caller, ctx.memory);
        let res = ctx
            .manager
            .write(&mut memory, write_buffer, buffer, &mut gas_left);
        state.ext.set_gas_left(gas_left);

        res.map_err(Into::<TerminationReason>::into)
    }

    #[host(cost = RuntimeCosts::Size)]
    pub fn size(ctx: CallerWrap<E>, length_ptr: u32) -> Result<()> {
        let size = ctx.host_state_mut().ext.size()? as u32;

        let write_size = ctx.register_write_as(length_ptr);
        ctx.write_as(write_size, size.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::Exit)]
    pub fn exit(ctx: CallerWrap<E>, inheritor_id_ptr: u32) -> Result<()> {
        let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);
        let inheritor_id = ctx.read_decoded(read_inheritor_id)?;

        Result::<(), ActorTerminationReason>::Err(ActorTerminationReason::Exit(inheritor_id))
    }

    #[host(fallible, cost = RuntimeCosts::StatusCode, err_len = LengthWithCode)]
    pub fn status_code(ctx: CallerWrap<E>) -> Result<StatusCode> {
        let state = ctx.host_state_mut();
        state.ext.status_code()
    }

    #[host(state_taken, cost = RuntimeCosts::Alloc)]
    pub fn alloc(ctx: CallerWrap<E>, state: &mut State<E>, pages: u32) -> FnResult<u32> {
        let mut mem = CallerWrap::memory(&mut ctx.caller, ctx.memory);

        let res = state.ext.alloc(pages, &mut mem);
        let res = state.process_alloc_func_result(res)?;
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

        Result::<(u32,), TerminationReason>::Ok((page,))
    }

    #[host(state_taken, cost = RuntimeCosts::Free)]
    pub fn free(ctx: CallerWrap<E>, state: &mut State<E>, page: u32) -> FnResult<i32> {
        let page =
            WasmPage::new(page).map_err(|_| TerminationReason::from(TrapExplanation::Unknown))?;

        let res = state.ext.free(page);
        let res = state.process_alloc_func_result(res)?;

        match &res {
            Ok(()) => {
                log::trace!("Free {page:?}");
            }
            Err(err) => {
                log::trace!("Free failed: {err}");
            }
        };

        Result::<(i32,), TerminationReason>::Ok((res.is_err() as i32,))
    }

    #[host(cost = RuntimeCosts::BlockHeight)]
    pub fn block_height(ctx: CallerWrap<E>, height_ptr: u32) -> Result<()> {
        let height = ctx.host_state_mut().ext.block_height()?;

        let write_height = ctx.register_write_as(height_ptr);
        ctx.write_as(write_height, height.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::BlockTimestamp)]
    pub fn block_timestamp(ctx: CallerWrap<E>, timestamp_ptr: u32) -> Result<()> {
        let timestamp = ctx.host_state_mut().ext.block_timestamp()?;

        let write_timestamp = ctx.register_write_as(timestamp_ptr);
        ctx.write_as(write_timestamp, timestamp.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::Origin)]
    pub fn origin(ctx: CallerWrap<E>, origin_ptr: u32) -> Result<()> {
        let origin = ctx.host_state_mut().ext.origin()?;

        let write_origin = ctx.register_write_as(origin_ptr);
        ctx.write_as(write_origin, origin.into_bytes())
    }

    #[host(fallible, wgas, cost = RuntimeCosts::Reply(len))]
    pub fn reply(
        ctx: CallerWrap<E>,
        payload_ptr: u32,
        len: u32,
        value_ptr: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let value = ctx.register_and_read_value(value_ptr)?;
        let payload = ctx.read(read_payload)?.try_into()?;

        let state = ctx.host_state_mut();
        state.ext.reply(ReplyPacket::new(payload, value))
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyCommit)]
    pub fn reply_commit(ctx: CallerWrap<E>, value_ptr: u32, delay: u32) -> Result<MessageId> {
        let value = ctx.register_and_read_value(value_ptr)?;

        let state = ctx.host_state_mut();
        state
            .ext
            .reply_commit(ReplyPacket::new(Default::default(), value))
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReply(len))]
    pub fn reservation_reply(
        ctx: CallerWrap<E>,
        rid_value_ptr: u32,
        payload_ptr: u32,
        len: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_rid_value = ctx.register_read_as(rid_value_ptr);
        let read_payload = ctx.register_read(payload_ptr, len);

        let HashWithValue {
            hash: reservation_id,
            value,
        } = ctx.read_as(read_rid_value)?;

        let payload = ctx.read(read_payload)?.try_into()?;
        let state = ctx.host_state_mut();
        state
            .ext
            .reservation_reply(reservation_id.into(), ReplyPacket::new(payload, value))
    }

    #[host(fallible, cost = RuntimeCosts::ReservationReplyCommit)]
    pub fn reservation_reply_commit(
        ctx: CallerWrap<E>,
        rid_value_ptr: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_rid_value = ctx.register_read_as(rid_value_ptr);

        let HashWithValue {
            hash: reservation_id,
            value,
        } = ctx.read_as(read_rid_value)?;

        let state = ctx.host_state_mut();
        state.ext.reservation_reply_commit(
            reservation_id.into(),
            ReplyPacket::new(Default::default(), value),
        )
    }

    #[host(fallible, cost = RuntimeCosts::ReplyTo)]
    pub fn reply_to(ctx: CallerWrap<E>) -> Result<MessageId> {
        let state = ctx.host_state_mut();
        state.ext.reply_to()
    }

    #[host(fallible, cost = RuntimeCosts::SignalFrom)]
    pub fn signal_from(ctx: CallerWrap<E>) -> Result<MessageId> {
        let state = ctx.host_state_mut();
        state.ext.signal_from()
    }

    #[host(fallible, cost = RuntimeCosts::ReplyPush(len), err_len = LengthBytes)]
    pub fn reply_push(ctx: CallerWrap<E>, payload_ptr: u32, len: u32) -> Result<()> {
        let read_payload = ctx.register_read(payload_ptr, len);
        let payload = ctx.read(read_payload)?;

        let state = ctx.host_state_mut();
        state.ext.reply_push(&payload)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::ReplyInput)]
    pub fn reply_input(
        ctx: CallerWrap<E>,
        offset: u32,
        len: u32,
        value_ptr: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let value = ctx.register_and_read_value(value_ptr)?;
        let state = ctx.host_state_mut();

        let mut f = || {
            state.ext.reply_push_input(offset, len)?;
            state
                .ext
                .reply_commit(ReplyPacket::new(Default::default(), value))
        };
        f()
    }

    #[host(fallible, cost = RuntimeCosts::ReplyPushInput, err_len = LengthBytes)]
    pub fn reply_push_input(ctx: CallerWrap<E>, offset: u32, len: u32) -> Result<()> {
        let state = ctx.host_state_mut();
        state.ext.reply_push_input(offset, len)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::SendInput)]
    pub fn send_input(
        ctx: CallerWrap<E>,
        pid_value_ptr: u32,
        offset: u32,
        len: u32,
        delay: u32,
    ) -> Result<MessageId> {
        let read_pid_value = ctx.register_read_as(pid_value_ptr);

        let HashWithValue {
            hash: destination,
            value,
        } = ctx.read_as(read_pid_value)?;

        let state = ctx.host_state_mut();
        let mut f = || {
            let handle = state.ext.send_init()?;
            state.ext.send_push_input(handle, offset, len)?;
            state.ext.send_commit(
                handle,
                HandlePacket::new(destination.into(), Default::default(), value),
                delay,
            )
        };

        f()
    }

    #[host(fallible, cost = RuntimeCosts::SendPushInput, err_len = LengthBytes)]
    pub fn send_push_input(ctx: CallerWrap<E>, handle: u32, offset: u32, len: u32) -> Result<()> {
        let state = ctx.host_state_mut();
        state.ext.send_push_input(handle, offset, len)
    }

    #[host(cost = RuntimeCosts::Debug(len))]
    pub fn debug(ctx: CallerWrap<E>, string_ptr: u32, len: u32) -> Result<()> {
        let read_data = ctx.register_read(string_ptr, len);

        let data: RuntimeBuffer = ctx.read(read_data)?.try_into()?;

        let s = String::from_utf8(data.into_vec())?;
        ctx.host_state_mut().ext.debug(&s)
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn panic(ctx: CallerWrap<E>, string_ptr: u32, len: u32) -> Result<()> {
        let read_data = ctx.register_read(string_ptr, len);
        let data = ctx.read(read_data).unwrap_or_default();

        let s = String::from_utf8_lossy(&data).to_string();

        Result::<(), TerminationReason>::Err(
            ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into(),
        )
    }

    #[host(cost = RuntimeCosts::Null)]
    pub fn oom_panic(ctx: CallerWrap<E>) -> Result<()> {
        Result::<(), TerminationReason>::Err(
            ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into(),
        )
    }

    #[host(fallible, cost = RuntimeCosts::ReserveGas)]
    pub fn reserve_gas(ctx: CallerWrap<E>, gas: u64, duration: u32) -> Result<ReservationId> {
        let state = ctx.host_state_mut();
        state.ext.reserve_gas(gas, duration)
    }

    #[host(fallible, cost = RuntimeCosts::UnreserveGas, err_len = LengthWithGas)]
    pub fn unreserve_gas(ctx: CallerWrap<E>, reservation_id_ptr: u32) -> Result<u64> {
        let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
        let id = ctx.read_decoded(read_reservation_id)?;

        let state = ctx.host_state_mut();
        state.ext.unreserve_gas(id)
    }

    #[host(fallible, cost = RuntimeCosts::SystemReserveGas, err_len = LengthBytes)]
    pub fn system_reserve_gas(ctx: CallerWrap<E>, gas: u64) -> Result<()> {
        let state = ctx.host_state_mut();
        state.ext.system_reserve_gas(gas)
    }

    #[host(cost = RuntimeCosts::GasAvailable)]
    pub fn gas_available(ctx: CallerWrap<E>, gas_ptr: u32) -> Result<()> {
        let gas = ctx.host_state_mut().ext.gas_available()?;

        let write_gas = ctx.register_write_as(gas_ptr);
        ctx.write_as(write_gas, gas.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::MsgId)]
    pub fn message_id(ctx: CallerWrap<E>, message_id_ptr: u32) -> Result<()> {
        let message_id = ctx.host_state_mut().ext.message_id()?;

        let write_message_id = ctx.register_write_as(message_id_ptr);
        ctx.write_as(write_message_id, message_id.into_bytes())
    }

    #[host(fallible, cost = RuntimeCosts::PayProgramRent, err_len = LengthWithBlockNumberAndValue)]
    pub fn pay_program_rent(ctx: CallerWrap<E>, rent_pid_ptr: u32) -> Result<(u128, u32)> {
        let read_rent_pid = ctx.register_read_as(rent_pid_ptr);

        let HashWithValue {
            hash: program_id,
            value: rent,
        } = ctx.read_as(read_rent_pid)?;

        let state = ctx.host_state_mut();
        state.ext.pay_program_rent(program_id.into(), rent)
    }

    #[host(cost = RuntimeCosts::ProgramId)]
    pub fn program_id(ctx: CallerWrap<E>, program_id_ptr: u32) -> Result<()> {
        let program_id = ctx.host_state_mut().ext.program_id()?;

        let write_program_id = ctx.register_write_as(program_id_ptr);
        ctx.write_as(write_program_id, program_id.into_bytes())
    }

    #[host(cost = RuntimeCosts::Source)]
    pub fn source(ctx: CallerWrap<E>, source_ptr: u32) -> Result<()> {
        let source = ctx.host_state_mut().ext.source()?;

        let write_source = ctx.register_write_as(source_ptr);
        ctx.write_as(write_source, source.into_bytes())
    }

    #[host(cost = RuntimeCosts::Value)]
    pub fn value(ctx: CallerWrap<E>, value_ptr: u32) -> Result<()> {
        let value = ctx.host_state_mut().ext.value()?;

        let write_value = ctx.register_write_as(value_ptr);
        ctx.write_as(write_value, value.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::ValueAvailable)]
    pub fn value_available(ctx: CallerWrap<E>, value_ptr: u32) -> Result<()> {
        let value_available = ctx.host_state_mut().ext.value_available()?;

        let write_value = ctx.register_write_as(value_ptr);
        ctx.write_as(write_value, value_available.to_le_bytes())
    }

    #[host(cost = RuntimeCosts::Random)]
    pub fn random(ctx: CallerWrap<E>, subject_ptr: u32, bn_random_ptr: u32) -> Result<()> {
        let read_subject = ctx.register_read_decoded(subject_ptr);
        let write_bn_random = ctx.register_write_as::<BlockNumberWithHash>(bn_random_ptr);

        let raw_subject: Hash = ctx.read_decoded(read_subject)?;

        let (random, bn) = ctx.host_state_mut().ext.random()?;
        let subject = [&raw_subject, random].concat();

        let mut hash = [0; 32];
        hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

        ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
    }

    #[host(cost = RuntimeCosts::Leave)]
    pub fn leave(ctx: CallerWrap<E>) -> Result<()> {
        Result::<(), TerminationReason>::Err(ActorTerminationReason::Leave.into())
    }

    #[host(cost = RuntimeCosts::Wait)]
    pub fn wait(ctx: CallerWrap<E>) -> Result<()> {
        ctx.host_state_mut().ext.wait()?;
        Result::<(), TerminationReason>::Err(
            ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into(),
        )
    }

    #[host(cost = RuntimeCosts::WaitFor)]
    pub fn wait_for(ctx: CallerWrap<E>, duration: u32) -> Result<()> {
        ctx.host_state_mut().ext.wait_for(duration)?;
        Result::<(), TerminationReason>::Err(
            ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into(),
        )
    }

    #[host(cost = RuntimeCosts::WaitUpTo)]
    pub fn wait_up_to(ctx: CallerWrap<E>, duration: u32) -> Result<()> {
        let waited_type = if ctx.host_state_mut().ext.wait_up_to(duration)? {
            MessageWaitedType::WaitUpToFull
        } else {
            MessageWaitedType::WaitUpTo
        };
        Result::<(), TerminationReason>::Err(
            ActorTerminationReason::Wait(Some(duration), waited_type).into(),
        )
    }

    #[host(fallible, cost = RuntimeCosts::Wake, err_len = LengthBytes)]
    pub fn wake(ctx: CallerWrap<E>, message_id_ptr: u32, delay: u32) -> Result<()> {
        let read_message_id = ctx.register_read_decoded(message_id_ptr);

        let message_id = ctx.read_decoded(read_message_id)?;

        let state = ctx.host_state_mut();
        state.ext.wake(message_id, delay)
    }

    #[host(fallible, wgas, cost = RuntimeCosts::CreateProgram(payload_len, salt_len), err_len = LengthWithTwoHashes)]
    pub fn create_program(
        ctx: CallerWrap<E>,
        cid_value_ptr: u32,
        salt_ptr: u32,
        salt_len: u32,
        payload_ptr: u32,
        payload_len: u32,
        delay: u32,
    ) -> Result<(MessageId, ProgramId)> {
        let read_cid_value = ctx.register_read_as(cid_value_ptr);
        let read_salt = ctx.register_read(salt_ptr, salt_len);
        let read_payload = ctx.register_read(payload_ptr, payload_len);

        let HashWithValue {
            hash: code_id,
            value,
        } = ctx.read_as(read_cid_value)?;
        let salt = ctx.read(read_salt)?.try_into()?;
        let payload = ctx.read(read_payload)?.try_into()?;

        let state = ctx.host_state_mut();
        state
            .ext
            .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
    }

    #[host(fallible, cost = RuntimeCosts::Error, err_len = LengthBytes)]
    pub fn error(ctx: CallerWrap<E>, err_buf_ptr: u32) -> Result<()> {
        let state = ctx.host_state_mut();

        if let Some(err) = state.fallible_syscall_error.as_ref() {
            let err = err.encode();
            let write_error_bytes = ctx.register_write(err_buf_ptr, err.len() as u32);
            ctx.write(write_error_bytes, err.as_ref())
                .map_err(Into::into)
        } else {
            Result::<(), TerminationReason>::Err(
                ActorTerminationReason::Trap(TrapExplanation::Ext(ExtError::SyscallUsage)).into(),
            )
        }
    }

    pub fn out_of_gas(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            syscall_trace!("out_of_gas");
            let host_state = internal::caller_host_state_mut(&mut caller);
            host_state.set_termination_reason(
                ActorTerminationReason::Trap(TrapExplanation::GasLimitExceeded).into(),
            );
            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn out_of_allowance(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            syscall_trace!("out_of_allowance");
            let host_state = internal::caller_host_state_mut(&mut caller);
            host_state.set_termination_reason(ActorTerminationReason::GasAllowanceExceeded.into());
            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }
}
