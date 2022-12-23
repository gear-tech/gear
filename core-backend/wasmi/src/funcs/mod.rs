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
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use blake2_rfc::blake2b::blake2b;
use codec::{Decode, Encode};
use core::{
    convert::TryInto,
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::Range,
};
use gear_backend_common::{
    error_processor::{IntoExtError, ProcessError},
    memory::MemoryAccessError,
    AsTerminationReason, IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    buffer::RuntimeBufferSizeError,
    env::Ext,
    memory::{PageU32Size, WasmPageNumber},
    message::{HandlePacket, InitPacket, MessageWaitedType, PayloadSizeError, ReplyPacket},
};
use gear_core_errors::{CoreError, MemoryError};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthWithCode, LengthWithGas, LengthWithHandle,
    LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
};

const PTR_SPECIAL: u32 = i32::MAX as u32;

#[derive(Debug, derive_more::Display, derive_more::From, Encode, Decode)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{_0}")]
    Core(E),
    #[display(fmt = "Runtime Error")]
    HostError,
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{_0}")]
    PayloadSize(PayloadSizeError),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "{_0}")]
    PayloadBufferSize(PayloadSizeError),
    #[display(fmt = "Failed to parse debug string")]
    DebugStringParsing,
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
    #[display(fmt = "Cannot take data by indexes {_0:?} from message with size {_1}")]
    ReadWrongRange(Range<u32>, u32),
    #[display(fmt = "Overflow at {_0} + len {_1} in `gr_read`")]
    ReadLenOverflow(u32, u32),
    DecodeValueError,
    DebugString,
}

impl<E: Display> From<MemoryAccessError> for FuncError<E> {
    fn from(err: MemoryAccessError) -> Self {
        match err {
            MemoryAccessError::Memory(err) => Self::Memory(err),
            MemoryAccessError::RuntimeBuffer(err) => Self::RuntimeBufferSize(err),
            MemoryAccessError::DecodeError => Self::DecodeValueError,
        }
    }
}

impl<E> FuncError<E>
where
    E: Display,
{
    pub fn into_termination_reason(self) -> TerminationReason {
        match self {
            Self::Terminated(reason) => reason,
            err => TerminationReason::Trap(TrapExplanation::Other(err.to_string().into())),
        }
    }
}

impl<E: Display> From<MemoryError> for FuncError<E> {
    fn from(err: MemoryError) -> Self {
        Self::Memory(err)
    }
}

pub struct FuncsHandler<E: Ext + 'static> {
    _phantom: PhantomData<E>,
}

type FnResult<T> = Result<(T,), Trap>;
type EmptyOutput = Result<(), Trap>;

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: Encode + AsTerminationReason + IntoExtError,
{
    pub fn send(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         pid_value_ptr: u32,
                         payload_ptr: u32,
                         len: u32,
                         delay: u32,
                         err_mid_ptr: u32|
              -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_hash_val = ctx.add_read_as(pid_value_ptr);
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_hash_val)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .send(HandlePacket::new(destination.into(), payload, value), delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_hash_val = ctx.add_read_as(pid_value_ptr);
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_hash_val)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .send(
                        HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_pid_value = ctx.add_read_as(pid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                ctx.host_state_mut()
                    .ext
                    .send_commit(
                        handle,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_pid_value = ctx.add_read_as(pid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                ctx.host_state_mut()
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
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn send_init(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_handle_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_handle = ctx.add_write_as(err_handle_ptr);

                ctx.host_state_mut()
                    .ext
                    .send_init()
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_handle, LengthWithHandle::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_len = ctx.add_write_as(err_ptr);

                let payload = ctx.read(read_payload)?;

                let len = ctx
                    .host_state_mut()
                    .ext
                    .send_push(handle, &payload)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, len.to_le_bytes())
                    .map_err(Into::into)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_rid_pid_value = ctx.add_read_as(rid_pid_value_ptr);
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .reservation_send(
                        reservation_id.into(),
                        HandlePacket::new(destination.into(), payload, value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_rid_pid_value = ctx.add_read_as(rid_pid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;

                ctx.host_state_mut()
                    .ext
                    .reservation_send_commit(
                        reservation_id.into(),
                        handle,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
        };

        Func::wrap(store, func)
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

    pub fn read(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         at: u32,
                         len: u32,
                         buffer_ptr: u32,
                         err_len_ptr: u32|
              -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_state_taken(|ctx, state| {
                let write_err_len = ctx.add_write_as(err_len_ptr);

                let length = if let Ok(buffer) = Self::validated(&mut state.ext, at, len) {
                    let write_buffer = ctx.add_write(buffer_ptr, len);
                    ctx.write(write_buffer, buffer)?;
                    0u32
                } else {
                    // TODO: issue #1652.
                    1u32
                };

                ctx.write_as(write_err_len, length.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, length_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_size = ctx.add_write_as(length_ptr);

                let size = ctx.host_state_mut().ext.size().map_err(FuncError::Core)? as u32;

                ctx.write_as(write_size, size.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, inheritor_id_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| -> Result<(), _> {
                let read_inheritor_id = ctx.add_read_decoded(inheritor_id_ptr);

                let inheritor_id = ctx.read_decoded(read_inheritor_id)?;

                ctx.host_state_mut().ext.exit().map_err(FuncError::Core)?;

                Err(FuncError::Terminated(TerminationReason::Exit(inheritor_id)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_code = ctx.add_write_as(err_code_ptr);

                ctx.host_state_mut()
                    .ext
                    .status_code()
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_code, LengthWithCode::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
            let pages =
                WasmPageNumber::new(pages).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run_state_taken(|ctx, state| {
                let mut mem = ctx.memory();
                let page = state.ext.alloc(pages, &mut mem).map_err(FuncError::Core)?;
                log::debug!("Alloc {:?} pages at {:?}", pages, page);
                Ok((page.raw(),))
            })
        };

        Func::wrap(store, func)
    }

    pub fn free(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, page: u32| -> EmptyOutput {
            let page = WasmPageNumber::new(page).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                ctx.host_state_mut()
                    .ext
                    .free(page)
                    .map(|_| log::debug!("Free {:?}", page))
                    .map_err(FuncError::Core)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_height = ctx.add_write_as(height_ptr);

                let height = ctx
                    .host_state_mut()
                    .ext
                    .block_height()
                    .map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_timestamp = ctx.add_write_as(timestamp_ptr);

                let timestamp = ctx
                    .host_state_mut()
                    .ext
                    .block_timestamp()
                    .map_err(FuncError::Core)?;

                ctx.write_as(write_timestamp, timestamp.to_le_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, origin_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_origin = ctx.add_write_as(origin_ptr);

                let origin = ctx.host_state_mut().ext.origin().map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .reply(ReplyPacket::new(payload, value), delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                ctx.host_state_mut()
                    .ext
                    .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                ctx.host_state_mut()
                    .ext
                    .reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_rid_value = ctx.add_read_as(rid_value_ptr);
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .reservation_reply(
                        reservation_id.into(),
                        ReplyPacket::new(payload, value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_rid_value = ctx.add_read_as(rid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;

                ctx.host_state_mut()
                    .ext
                    .reservation_reply_commit(
                        reservation_id.into(),
                        ReplyPacket::new(Default::default(), value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_mid_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                ctx.host_state_mut()
                    .ext
                    .reply_to()
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                ctx.host_state_mut()
                    .ext
                    .signal_from()
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_payload = ctx.add_read(payload_ptr, len);
                let write_err_len = ctx.add_write_as(err_ptr);

                let payload = ctx.read(read_payload)?;

                let len = ctx
                    .host_state_mut()
                    .ext
                    .reply_push(&payload)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, len.to_le_bytes())
                    .map_err(Into::into)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let push_result = ctx.host_state_mut().ext.reply_push_input(offset, len);
                push_result
                    .and_then(|_| {
                        ctx.host_state_mut()
                            .ext
                            .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                    })
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_len = ctx.add_write_as(err_ptr);

                let result_len = ctx
                    .host_state_mut()
                    .ext
                    .reply_push_input(offset, len)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, result_len.to_le_bytes())
                    .map_err(Into::into)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.add_read_decoded(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let push_result = ctx.host_state_mut().ext.reply_push_input(offset, len);
                push_result
                    .and_then(|_| {
                        ctx.host_state_mut().ext.reply_commit(
                            ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                            delay,
                        )
                    })
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_pid_value = ctx.add_read_as(pid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                let handle = ctx.host_state_mut().ext.send_init();
                let push_result = handle.and_then(|h| {
                    ctx.host_state_mut()
                        .ext
                        .send_push_input(h, offset, len)
                        .map(|_| h)
                });
                push_result
                    .and_then(|h| {
                        ctx.host_state_mut().ext.send_commit(
                            h,
                            HandlePacket::new(destination.into(), Default::default(), value),
                            delay,
                        )
                    })
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_len = ctx.add_write_as(err_ptr);

                let result_len = ctx
                    .host_state_mut()
                    .ext
                    .send_push_input(handle, offset, len)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, result_len.to_le_bytes())
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_pid_value = ctx.add_read_as(pid_value_ptr);
                let write_err_mid = ctx.add_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                let handle = ctx.host_state_mut().ext.send_init();
                let push_result = handle.and_then(|h| {
                    ctx.host_state_mut()
                        .ext
                        .send_push_input(h, offset, len)
                        .map(|_| h)
                });
                push_result
                    .and_then(|h| {
                        ctx.host_state_mut().ext.send_commit(
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
                    .proc_res(|res| ctx.write_as(write_err_mid, LengthWithHash::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, string_ptr: u32, len: u32| -> EmptyOutput {
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run(|ctx| {
                    let read_data = ctx.add_read(string_ptr, len);

                    let data = ctx.read(read_data)?;

                    let s = String::from_utf8(data).map_err(|_| FuncError::DebugString)?;
                    ctx.host_state_mut()
                        .ext
                        .debug(&s)
                        .map_err(FuncError::Core)?;

                    Ok(())
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_rid = ctx.add_write_as(err_rid_ptr);

                ctx.host_state_mut()
                    .ext
                    .reserve_gas(gas, duration)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_rid, LengthWithHash::from(res)))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_reservation_id = ctx.add_read_decoded(reservation_id_ptr);
                let write_err_unreserved = ctx.add_write_as(err_unreserved_ptr);

                let id = ctx.read_decoded(read_reservation_id)?;

                ctx.host_state_mut()
                    .ext
                    .unreserve_gas(id)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_unreserved, LengthWithGas::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn system_reserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, gas: u64, err_ptr: u32| {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_len = ctx.add_write_as(err_ptr);

                let len = ctx
                    .host_state_mut()
                    .ext
                    .system_reserve_gas(gas)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, len.to_le_bytes())
                    .map_err(Into::into)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_gas = ctx.add_write_as(gas_ptr);

                let gas = ctx
                    .host_state_mut()
                    .ext
                    .gas_available()
                    .map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_message_id = ctx.add_write_as(message_id_ptr);

                let message_id = ctx
                    .host_state_mut()
                    .ext
                    .message_id()
                    .map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_program_id = ctx.add_write_as(program_id_ptr);

                let program_id = ctx
                    .host_state_mut()
                    .ext
                    .program_id()
                    .map_err(FuncError::Core)?;

                ctx.write_as(write_program_id, program_id.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, source_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_source = ctx.add_write_as(source_ptr);

                let source = ctx.host_state_mut().ext.source().map_err(FuncError::Core)?;

                ctx.write_as(write_source, source.into_bytes())
                    .map_err(Into::into)
            })
        };

        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_value = ctx.add_write_as(value_ptr);

                let value = ctx.host_state_mut().ext.value().map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_value = ctx.add_write_as(value_ptr);

                let value_available = ctx
                    .host_state_mut()
                    .ext
                    .value_available()
                    .map_err(FuncError::Core)?;

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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_subject = ctx.add_read_decoded(subject_ptr);
                let write_bn_random = ctx.add_write_as::<BlockNumberWithHash>(bn_random_ptr);

                let raw_subject: Hash = ctx.read_decoded(read_subject)?;

                let (random, bn) = ctx.host_state_mut().ext.random().map_err(FuncError::Core)?;
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| -> Result<(), _> {
                Err(ctx
                    .host_state_mut()
                    .ext
                    .leave()
                    .map_err(FuncError::Core)
                    .err()
                    .unwrap_or_else(|| FuncError::Terminated(TerminationReason::Leave)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| -> Result<(), _> {
                Err(ctx
                    .host_state_mut()
                    .ext
                    .wait()
                    .map_err(FuncError::Core)
                    .err()
                    .unwrap_or_else(|| {
                        FuncError::Terminated(TerminationReason::Wait(
                            None,
                            MessageWaitedType::Wait,
                        ))
                    }))
            })
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| -> Result<(), _> {
                Err(ctx
                    .host_state_mut()
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
        };

        Func::wrap(store, func)
    }

    pub fn wait_up_to(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| -> Result<(), _> {
                Err(FuncError::Terminated(TerminationReason::Wait(
                    Some(duration),
                    if ctx
                        .host_state_mut()
                        .ext
                        .wait_up_to(duration)
                        .map_err(FuncError::Core)?
                    {
                        MessageWaitedType::WaitUpToFull
                    } else {
                        MessageWaitedType::WaitUpTo
                    },
                )))
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_message_id = ctx.add_read_decoded(message_id_ptr);
                let write_err_len = ctx.add_write_as(err_ptr);

                let message_id = ctx.read_decoded(read_message_id)?;

                let len = ctx
                    .host_state_mut()
                    .ext
                    .wake(message_id, delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .error_len();

                ctx.write_as(write_err_len, len.to_le_bytes())
                    .map_err(Into::into)
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_cid_value = ctx.add_read_as(cid_value_ptr);
                let read_salt = ctx.add_read(salt_ptr, salt_len);
                let read_payload = ctx.add_read(payload_ptr, payload_len);
                let write_err_mid_pid = ctx.add_write_as(err_mid_pid_ptr);

                let HashWithValue {
                    hash: code_id,
                    value,
                } = ctx.read_as(read_cid_value)?;
                let salt = ctx.read(read_salt)?.try_into()?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res)))
            })
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
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let read_cid_value = ctx.add_read_as(cid_value_ptr);
                let read_salt = ctx.add_read(salt_ptr, salt_len);
                let read_payload = ctx.add_read(payload_ptr, payload_len);
                let write_err_mid_pid = ctx.add_write_as(err_mid_pid_ptr);

                let HashWithValue {
                    hash: code_id,
                    value,
                } = ctx.read_as(read_cid_value)?;
                let salt = ctx.read(read_salt)?.try_into()?;
                let payload = ctx.read(read_payload)?.try_into()?;

                ctx.host_state_mut()
                    .ext
                    .create_program(
                        InitPacket::new_with_gas(code_id.into(), salt, payload, gas_limit, value),
                        delay,
                    )
                    .process_error()
                    .map_err(FuncError::Core)?
                    .proc_res(|res| ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res)))
            })
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, error_ptr: u32, err_ptr: u32| -> EmptyOutput {
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run(|ctx| {
                    ctx.host_state_mut()
                        .ext
                        .last_error_encoded()
                        .process_error()
                        .map_err(FuncError::Core)?
                        .proc_res(|res| -> Result<(), FuncError<E::Error>> {
                            let write_err_len = ctx.add_write_as(err_ptr);
                            let length = match res {
                                Ok(err) => {
                                    let write_error_bytes =
                                        ctx.add_write(error_ptr, err.len() as u32);
                                    ctx.write(write_error_bytes, err.as_ref())?;
                                    0
                                }
                                Err(length) => length,
                            };

                            ctx.host_state_mut()
                                .ext
                                .charge_error()
                                .map_err(FuncError::Core)?;
                            ctx.write_as(write_err_len, length.to_le_bytes())?;
                            Ok(())
                        })
                })
            };

        Func::wrap(store, func)
    }

    pub fn out_of_gas(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            let host_state = caller
                .host_data_mut()
                .as_mut()
                .expect("host_state should be set before execution");

            host_state.err = FuncError::Core(host_state.ext.out_of_gas());
            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn out_of_allowance(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            let host_state = caller
                .host_data_mut()
                .as_mut()
                .expect("host_state should be set before execution");

            host_state.ext.out_of_allowance();
            host_state.err = FuncError::Terminated(TerminationReason::GasAllowanceExceeded);

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }
}
