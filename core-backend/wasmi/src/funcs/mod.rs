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
    BlockNumberWithHash, Hash, HashWithValue, LengthBytes, LengthWithCode, LengthWithGas,
    LengthWithHandle, LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
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
    pub fn send(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         pid_value_ptr: u32,
                         payload_ptr: u32,
                         len: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("send", pid_value_ptr, payload_ptr, len, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Send(len), |ctx| {
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
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         pid_value_ptr: u32,
                         payload_ptr: u32,
                         len: u32,
                         gas_limit: u64,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "send",
                pid_value_ptr,
                payload_ptr,
                len,
                gas_limit,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Send(len), |ctx| {
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
                    .send(
                        HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                        delay,
                    )
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         handle: u32,
                         pid_value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("send_commit", handle, pid_value_ptr, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(
                err_mid_ptr,
                RuntimeCosts::SendCommit(0),
                |ctx| {
                    let read_pid_value = ctx.register_read_as(pid_value_ptr);

                    let HashWithValue {
                        hash: destination,
                        value,
                    } = ctx.read_as(read_pid_value)?;

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .send_commit(
                            handle,
                            HandlePacket::new(destination.into(), Default::default(), value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_commit_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         handle: u32,
                         pid_value_ptr: u32,
                         gas_limit: u64,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "send_commit_wgas",
                handle,
                pid_value_ptr,
                gas_limit,
                delay,
                err_mid_ptr
            );

            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(
                err_mid_ptr,
                RuntimeCosts::SendCommit(0),
                |ctx| {
                    let read_pid_value = ctx.register_read_as(pid_value_ptr);

                    let HashWithValue {
                        hash: destination,
                        value,
                    } = ctx.read_as(read_pid_value)?;

                    let state = ctx.host_state_mut();
                    state
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
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_init(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_handle_ptr: u32| -> EmptyOutput {
            syscall_trace!("send_init", err_handle_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHandle>(
                err_handle_ptr,
                RuntimeCosts::SendInit,
                |ctx| {
                    let state = ctx.host_state_mut();
                    state.ext.send_init().map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_push(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         handle: u32,
                         payload_ptr: u32,
                         len: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("send_push", handle, payload_ptr, len, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::SendPush(len), |ctx| {
                let read_payload = ctx.register_read(payload_ptr, len);
                let payload = ctx.read(read_payload)?;

                let state = ctx.host_state_mut();
                state.ext.send_push(handle, &payload).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reservation_send(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         rid_pid_value_ptr: u32,
                         payload_ptr: u32,
                         len: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reservation_send",
                rid_pid_value_ptr,
                payload_ptr,
                len,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

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

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reservation_send(
                            reservation_id.into(),
                            HandlePacket::new(destination.into(), payload, value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reservation_send_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         handle: u32,
                         rid_pid_value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reservation_send_commit",
                handle,
                rid_pid_value_ptr,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

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

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reservation_send_commit(
                            reservation_id.into(),
                            handle,
                            HandlePacket::new(destination.into(), Default::default(), value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         at: u32,
                         len: u32,
                         buffer_ptr: u32,
                         err_len_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("read", at, len, buffer_ptr, err_len_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible_state_taken::<_, _, LengthBytes>(
                err_len_ptr,
                RuntimeCosts::Read,
                |ctx, state| {
                    // State is taken, so we cannot use `MemoryOwner` functions from `CallerWrap`.
                    let (buffer, mut gas_left) = state.ext.read(at, len)?;

                    let write_buffer = ctx.register_write(buffer_ptr, len);

                    let mut memory = CallerWrap::memory(&mut ctx.caller, ctx.memory);
                    ctx.manager
                        .write(&mut memory, write_buffer, buffer, &mut gas_left)?;

                    state.ext.set_gas_left(gas_left);

                    Ok(())
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, length_ptr: u32| -> EmptyOutput {
            syscall_trace!("size", length_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Size, |ctx| {
                let size = ctx.host_state_mut().ext.size()? as u32;

                let write_size = ctx.register_write_as(length_ptr);
                ctx.write_as(write_size, size.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, inheritor_id_ptr: u32| -> EmptyOutput {
            syscall_trace!("exit", inheritor_id_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Exit, |ctx| -> Result<(), _> {
                let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);
                let inheritor_id = ctx.read_decoded(read_inheritor_id)?;
                Err(ActorTerminationReason::Exit(inheritor_id).into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn status_code(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_code_ptr: u32| -> EmptyOutput {
            syscall_trace!("status_code", err_code_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithCode>(
                err_code_ptr,
                RuntimeCosts::StatusCode,
                |ctx| {
                    let state = ctx.host_state_mut();
                    state.ext.status_code().map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
            syscall_trace!("alloc", pages);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_state_taken(RuntimeCosts::Alloc, |ctx, state| {
                let mut mem = CallerWrap::memory(&mut ctx.caller, ctx.memory);

                // TODO: we must return u32::MAX here #2353
                let pages = WasmPage::new(pages).map_err(|_| TrapExplanation::Unknown)?;

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

                Ok((page,))
            })
        };

        Func::wrap(store, func)
    }

    pub fn free(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, page: u32| -> FnResult<i32> {
            syscall_trace!("free", page);
            let page = WasmPage::new(page).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_state_taken(RuntimeCosts::Free, |_ctx, state| {
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

                Ok((res.is_err() as i32,))
            })
        };

        Func::wrap(store, func)
    }

    pub fn block_height(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, height_ptr: u32| -> EmptyOutput {
            syscall_trace!("block_height", height_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::BlockHeight, |ctx| {
                let height = ctx.host_state_mut().ext.block_height()?;

                let write_height = ctx.register_write_as(height_ptr);
                ctx.write_as(write_height, height.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn block_timestamp(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, timestamp_ptr: u32| -> EmptyOutput {
            syscall_trace!("block_timestamp", timestamp_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::BlockTimestamp, |ctx| {
                let timestamp = ctx.host_state_mut().ext.block_timestamp()?;

                let write_timestamp = ctx.register_write_as(timestamp_ptr);
                ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, origin_ptr: u32| -> EmptyOutput {
            syscall_trace!("origin", origin_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Origin, |ctx| {
                let origin = ctx.host_state_mut().ext.origin()?;

                let write_origin = ctx.register_write_as(origin_ptr);
                ctx.write_as(write_origin, origin.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         len: u32,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reply", payload_ptr, len, value_ptr, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Reply(len), |ctx| {
                let read_payload = ctx.register_read(payload_ptr, len);
                let value = ctx.register_and_read_value(value_ptr)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                state
                    .ext
                    .reply(ReplyPacket::new(payload, value), delay)
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         len: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reply_wgas",
                payload_ptr,
                len,
                gas_limit,
                value_ptr,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::Reply(len), |ctx| {
                let read_payload = ctx.register_read(payload_ptr, len);
                let value = ctx.register_and_read_value(value_ptr)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                state
                    .ext
                    .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reply_commit", value_ptr, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(
                err_mid_ptr,
                RuntimeCosts::ReplyCommit(0),
                |ctx| {
                    let value = ctx.register_and_read_value(value_ptr)?;

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_commit_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reply_commit_wgas",
                gas_limit,
                value_ptr,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(
                err_mid_ptr,
                RuntimeCosts::ReplyCommit(0),
                |ctx| {
                    let value = ctx.register_and_read_value(value_ptr)?;

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reply_commit(
                            ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reservation_reply(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         rid_value_ptr: u32,
                         payload_ptr: u32,
                         len: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reservation_reply",
                rid_value_ptr,
                payload_ptr,
                len,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

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

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reservation_reply(
                            reservation_id.into(),
                            ReplyPacket::new(payload, value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reservation_reply_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         rid_value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reservation_reply_commit",
                rid_value_ptr,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(
                err_mid_ptr,
                RuntimeCosts::ReservationReplyCommit(0),
                |ctx| {
                    let read_rid_value = ctx.register_read_as(rid_value_ptr);

                    let HashWithValue {
                        hash: reservation_id,
                        value,
                    } = ctx.read_as(read_rid_value)?;

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .reservation_reply_commit(
                            reservation_id.into(),
                            ReplyPacket::new(Default::default(), value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_mid_ptr: u32| -> EmptyOutput {
            syscall_trace!("reply_to", err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyTo, |ctx| {
                let state = ctx.host_state_mut();
                state.ext.reply_to().map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn signal_from(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_mid_ptr: u32| -> EmptyOutput {
            syscall_trace!("signal_from", err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SignalFrom, |ctx| {
                let state = ctx.host_state_mut();
                state.ext.signal_from().map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_push(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         len: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reply_push", payload_ptr, len, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::ReplyPush(len), |ctx| {
                let read_payload = ctx.register_read(payload_ptr, len);
                let payload = ctx.read(read_payload)?;

                let state = ctx.host_state_mut();
                state.ext.reply_push(&payload).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_input(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         offset: u32,
                         len: u32,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reply_input", offset, len, value_ptr, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyInput, |ctx| {
                let value = ctx.register_and_read_value(value_ptr)?;

                let state = ctx.host_state_mut();
                let mut f = || {
                    state.ext.reply_push_input(offset, len)?;
                    state
                        .ext
                        .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                };

                f().map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_push_input(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         offset: u32,
                         len: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reply_push_input", offset, len, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::ReplyPushInput, |ctx| {
                let state = ctx.host_state_mut();
                state.ext.reply_push_input(offset, len).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_input_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         offset: u32,
                         len: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "reply_input_wgas",
                offset,
                len,
                gas_limit,
                value_ptr,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::ReplyInput, |ctx| {
                let value = ctx.register_and_read_value(value_ptr)?;

                let state = ctx.host_state_mut();
                let mut f = || {
                    state.ext.reply_push_input(offset, len)?;
                    state.ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                };

                f().map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_input(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         pid_value_ptr: u32,
                         offset: u32,
                         len: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("send_input", pid_value_ptr, offset, len, delay, err_mid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendInput, |ctx| {
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

                f().map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_push_input(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         handle: u32,
                         offset: u32,
                         len: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("send_push_input", handle, offset, len, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::SendPushInput, |ctx| {
                let state = ctx.host_state_mut();
                state
                    .ext
                    .send_push_input(handle, offset, len)
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_input_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         pid_value_ptr: u32,
                         offset: u32,
                         len: u32,
                         gas_limit: u64,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "send_push_wgas",
                pid_value_ptr,
                offset,
                len,
                gas_limit,
                delay,
                err_mid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_mid_ptr, RuntimeCosts::SendInput, |ctx| {
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
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, string_ptr: u32, len: u32| -> EmptyOutput {
                syscall_trace!("debug", string_ptr, len);
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run(RuntimeCosts::Debug(len), |ctx| {
                    let read_data = ctx.register_read(string_ptr, len);

                    let data: RuntimeBuffer = ctx.read(read_data)?.try_into()?;

                    let s = String::from_utf8(data.into_vec())?;
                    ctx.host_state_mut().ext.debug(&s).map_err(Into::into)
                })
            };

        Func::wrap(store, func)
    }

    pub fn panic(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, string_ptr: u32, len: u32| -> EmptyOutput {
                syscall_trace!("panic", string_ptr, len);
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run(RuntimeCosts::Null, |ctx| {
                    let read_data = ctx.register_read(string_ptr, len);
                    let data = ctx.read(read_data).unwrap_or_default();

                    let s = String::from_utf8_lossy(&data).to_string();

                    Err(ActorTerminationReason::Trap(TrapExplanation::Panic(s.into())).into())
                })
            };

        Func::wrap(store, func)
    }

    pub fn oom_panic(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            syscall_trace!("oom_panic");
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Null, |_ctx| {
                Err(ActorTerminationReason::Trap(TrapExplanation::ProgramAllocOutOfBounds).into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn reserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         gas: u64,
                         duration: u32,
                         err_rid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("reserve_gas", gas, duration, err_rid_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithHash>(err_rid_ptr, RuntimeCosts::ReserveGas, |ctx| {
                let state = ctx.host_state_mut();
                state.ext.reserve_gas(gas, duration).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn unreserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         reservation_id_ptr: u32,
                         err_unreserved_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("unreserve_gas", reservation_id_ptr, err_unreserved_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthWithGas>(
                err_unreserved_ptr,
                RuntimeCosts::UnreserveGas,
                |ctx| {
                    let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
                    let id = ctx.read_decoded(read_reservation_id)?;

                    let state = ctx.host_state_mut();
                    state.ext.unreserve_gas(id).map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn system_reserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, gas: u64, err_ptr: u32| {
            syscall_trace!("system_reserve_gas", gas, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::SystemReserveGas, |ctx| {
                let state = ctx.host_state_mut();
                state.ext.system_reserve_gas(gas).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn gas_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, gas_ptr: u32| -> EmptyOutput {
            syscall_trace!("gas_available", gas_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::GasAvailable, |ctx| {
                let gas = ctx.host_state_mut().ext.gas_available()?;

                let write_gas = ctx.register_write_as(gas_ptr);
                ctx.write_as(write_gas, gas.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn message_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, message_id_ptr: u32| -> EmptyOutput {
            syscall_trace!("message_id", message_id_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::MsgId, |ctx| {
                let message_id = ctx.host_state_mut().ext.message_id()?;

                let write_message_id = ctx.register_write_as(message_id_ptr);
                ctx.write_as(write_message_id, message_id.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn program_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, program_id_ptr: u32| -> EmptyOutput {
            syscall_trace!("program_id", program_id_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::ProgramId, |ctx| {
                let program_id = ctx.host_state_mut().ext.program_id()?;

                let write_program_id = ctx.register_write_as(program_id_ptr);
                ctx.write_as(write_program_id, program_id.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, source_ptr: u32| -> EmptyOutput {
            syscall_trace!("source", source_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Source, |ctx| {
                let source = ctx.host_state_mut().ext.source()?;

                let write_source = ctx.register_write_as(source_ptr);
                ctx.write_as(write_source, source.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
            syscall_trace!("value", value_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Value, |ctx| {
                let value = ctx.host_state_mut().ext.value()?;

                let write_value = ctx.register_write_as(value_ptr);
                ctx.write_as(write_value, value.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn value_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
            syscall_trace!("value_available", value_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::ValueAvailable, |ctx| {
                let value_available = ctx.host_state_mut().ext.value_available()?;

                let write_value = ctx.register_write_as(value_ptr);
                ctx.write_as(write_value, value_available.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn random(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         subject_ptr: u32,
                         bn_random_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("random", subject_ptr, bn_random_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Random, |ctx| {
                let read_subject = ctx.register_read_decoded(subject_ptr);
                let write_bn_random = ctx.register_write_as::<BlockNumberWithHash>(bn_random_ptr);

                let raw_subject: Hash = ctx.read_decoded(read_subject)?;

                let (random, bn) = ctx.host_state_mut().ext.random()?;
                let subject = [&raw_subject, random].concat();

                let mut hash = [0; 32];
                hash.copy_from_slice(blake2b(32, &[], &subject).as_bytes());

                ctx.write_as(write_bn_random, BlockNumberWithHash { bn, hash })
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            syscall_trace!("leave");
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Leave, |_ctx| -> Result<(), _> {
                Err(ActorTerminationReason::Leave.into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            syscall_trace!("wait");
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::Wait, |ctx| -> Result<(), _> {
                ctx.host_state_mut().ext.wait()?;
                Err(ActorTerminationReason::Wait(None, MessageWaitedType::Wait).into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            syscall_trace!("wait_for", duration);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::WaitFor, |ctx| -> Result<(), _> {
                ctx.host_state_mut().ext.wait_for(duration)?;
                Err(ActorTerminationReason::Wait(Some(duration), MessageWaitedType::WaitFor).into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn wait_up_to(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            syscall_trace!("wait_up_to", duration);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(RuntimeCosts::WaitUpTo, |ctx| -> Result<(), _> {
                let waited_type = if ctx.host_state_mut().ext.wait_up_to(duration)? {
                    MessageWaitedType::WaitUpToFull
                } else {
                    MessageWaitedType::WaitUpTo
                };
                Err(ActorTerminationReason::Wait(Some(duration), waited_type).into())
            })
        };

        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         message_id_ptr: u32,
                         delay: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("wake", message_id_ptr, delay, err_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_ptr, RuntimeCosts::Wake, |ctx| {
                let read_message_id = ctx.register_read_decoded(message_id_ptr);

                let message_id = ctx.read_decoded(read_message_id)?;

                let state = ctx.host_state_mut();
                state.ext.wake(message_id, delay).map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn create_program(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         cid_value_ptr: u32,
                         salt_ptr: u32,
                         salt_len: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         delay: u32,
                         err_mid_pid_ptr: u32|
              -> EmptyOutput {
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

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

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .create_program(
                            InitPacket::new(code_id.into(), salt, payload, value),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn create_program_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         cid_value_ptr: u32,
                         salt_ptr: u32,
                         salt_len: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         gas_limit: u64,
                         delay: u32,
                         err_mid_pid_ptr: u32|
              -> EmptyOutput {
            syscall_trace!(
                "create_program",
                cid_value_ptr,
                salt_ptr,
                salt_len,
                payload_ptr,
                payload_len,
                gas_limit,
                delay,
                err_mid_pid_ptr
            );
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

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

                    let state = ctx.host_state_mut();
                    state
                        .ext
                        .create_program(
                            InitPacket::new_with_gas(
                                code_id.into(),
                                salt,
                                payload,
                                gas_limit,
                                value,
                            ),
                            delay,
                        )
                        .map_err(Into::into)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         err_buf_ptr: u32,
                         err_len_ptr: u32|
              -> EmptyOutput {
            syscall_trace!("error", err_buf_ptr, err_len_ptr);
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_fallible::<_, _, LengthBytes>(err_len_ptr, RuntimeCosts::Error, |ctx| {
                let state = ctx.host_state_mut();

                if let Some(err) = state.fallible_syscall_error.as_ref() {
                    let err = err.encode();
                    let write_error_bytes = ctx.register_write(err_buf_ptr, err.len() as u32);
                    ctx.write(write_error_bytes, err.as_ref())
                        .map_err(Into::into)
                } else {
                    Err(
                        ActorTerminationReason::Trap(TrapExplanation::Ext(ExtError::SyscallUsage))
                            .into(),
                    )
                }
            })
        };

        Func::wrap(store, func)
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
