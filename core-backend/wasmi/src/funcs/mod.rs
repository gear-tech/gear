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

use crate::{
    funcs::internal::CallerWrap,
    memory::MemoryWrapRef,
    state::{HostState, State},
};
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
    memory::{MemoryAccessError, MemoryAccessRecorder, MemoryOwner},
    IntoExtError, IntoExtInfo, TerminationReason, TrapExplanation,
};
use gear_core::{
    buffer::RuntimeBufferSizeError,
    env::Ext,
    memory::{PageU32Size, WasmPage},
    message::{HandlePacket, InitPacket, MessageWaitedType, PayloadSizeError, ReplyPacket},
};
use gear_core_errors::{CoreError, ExtError, MemoryError};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use gsys::{
    BlockNumberWithHash, Hash, HashWithValue, LengthWithCode, LengthWithGas, LengthWithHandle,
    LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
};

// TODO: change it to u32::MAX (issue #2027)
const PTR_SPECIAL: u32 = i32::MAX as u32;

#[derive(Debug, Clone, derive_more::Display, derive_more::From)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{_0}")]
    Core(E),
    #[display(fmt = "Binary code has wrong instrumentation")]
    WrongInstrumentation,
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[from]
    #[display(fmt = "{_0}")]
    PayloadSize(PayloadSizeError),
    #[from]
    #[display(fmt = "{_0}")]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
    #[display(fmt = "Cannot decode value from memory")]
    DecodeValueError,
    #[display(fmt = "Failed to parse debug string")]
    DebugString,
    #[display(fmt = "Buffer size {_0} is not equal to pre-registered size {_1}")]
    WrongBufferSize(usize, u32),
}

impl<E: Display> From<MemoryAccessError> for FuncError<E> {
    fn from(err: MemoryAccessError) -> Self {
        match err {
            MemoryAccessError::Memory(err) => Self::Memory(err),
            MemoryAccessError::RuntimeBuffer(err) => Self::RuntimeBufferSize(err),
            MemoryAccessError::DecodeError => Self::DecodeValueError,
            MemoryAccessError::WrongBufferSize(buffer_size, size) => {
                Self::WrongBufferSize(buffer_size, size)
            }
        }
    }
}

impl<E> FuncError<E>
where
    E: Display + IntoExtError,
{
    pub fn into_termination_reason(self) -> TerminationReason {
        match self {
            Self::Core(err) => err.into_termination_reason(),
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

trait IntoExtErrorForResult<T, Err, Ext>
where
    Err: Display,
    Ext: gear_core::env::Ext,
{
    fn into_ext_error(self, state: &mut State<Ext>) -> Result<Result<T, u32>, FuncError<Err>>;
}

impl<T, Err, Ext> IntoExtErrorForResult<T, Err, Ext> for Result<T, Err>
where
    Err: IntoExtError + Display + Clone,
    Ext: gear_core::env::Ext<Error = Err>,
{
    fn into_ext_error(self, state: &mut State<Ext>) -> Result<Result<T, u32>, FuncError<Err>> {
        match self {
            Ok(value) => Ok(Ok(value)),
            Err(err) => {
                state.err = FuncError::Core(err.clone());
                match err.into_ext_error() {
                    Ok(ext_err) => Ok(Err(ext_err.encoded_size() as u32)),
                    Err(err) => Err(FuncError::Core(err)),
                }
            }
        }
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
    E::Error: Encode + IntoExtError + Clone,
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
                let read_hash_val = ctx.register_read_as(pid_value_ptr);
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_hash_val)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .send(HandlePacket::new(destination.into(), payload, value), delay)
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_hash_val = ctx.register_read_as(pid_value_ptr);
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_hash_val)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .send(
                        HandlePacket::new_with_gas(destination.into(), payload, gas_limit, value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_pid_value = ctx.register_read_as(pid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .send_commit(
                        handle,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_pid_value = ctx.register_read_as(pid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: destination,
                    value,
                } = ctx.read_as(read_pid_value)?;

                let state = ctx.host_state_mut();
                let res = state
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
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_handle = ctx.register_write_as(err_handle_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.send_init().into_ext_error(state)?;
                ctx.write_as(write_err_handle, LengthWithHandle::from(res))?;
                Ok(())
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
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_len = ctx.register_write_as(err_ptr);

                let payload = ctx.read(read_payload)?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .send_push(handle, &payload)
                    .into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

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
                let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reservation_send(
                        reservation_id.into(),
                        HandlePacket::new(destination.into(), payload, value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_rid_pid_value = ctx.register_read_as(rid_pid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = ctx.read_as(read_rid_pid_value)?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reservation_send_commit(
                        reservation_id.into(),
                        handle,
                        HandlePacket::new(destination.into(), Default::default(), value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
            })
        };

        Func::wrap(store, func)
    }

    fn validated(
        ext: &'_ mut E,
        at: u32,
        len: u32,
    ) -> Result<&'_ [u8], FuncError<<E as Ext>::Error>> {
        let msg = ext.read(at, len).map_err(FuncError::Core)?;

        // 'at' and 'len' correct and saturation checked in Ext::read
        debug_assert!(at.checked_add(len).is_some());
        debug_assert!((at + len) as usize == msg.len());

        Ok(msg)
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
                let write_err_len = ctx.register_write_as(err_len_ptr);

                let length = if let Ok(buffer) = Self::validated(&mut state.ext, at, len) {
                    let write_buffer = ctx.register_write(buffer_ptr, len);
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
                let write_size = ctx.register_write_as(length_ptr);

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
                let read_inheritor_id = ctx.register_read_decoded(inheritor_id_ptr);

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
                let write_err_code = ctx.register_write_as(err_code_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.status_code().into_ext_error(state)?;
                ctx.write_as(write_err_code, LengthWithCode::from(res))?;
                Ok(())
            })
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
            let pages = WasmPage::new(pages).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

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
            let page = WasmPage::new(page).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

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
                let write_height = ctx.register_write_as(height_ptr);

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
                let write_timestamp = ctx.register_write_as(timestamp_ptr);

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
                let write_origin = ctx.register_write_as(origin_ptr);

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
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reply(ReplyPacket::new(payload, value), delay)
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay)
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded::<u128>(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_rid_value = ctx.register_read_as(rid_value_ptr);
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;
                let payload = ctx.read(read_payload)?.try_into()?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reservation_reply(
                        reservation_id.into(),
                        ReplyPacket::new(payload, value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_rid_value = ctx.register_read_as(rid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = ctx.read_as(read_rid_value)?;

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reservation_reply_commit(
                        reservation_id.into(),
                        ReplyPacket::new(Default::default(), value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
            })
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_mid_ptr: u32| -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.reply_to().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.signal_from().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_payload = ctx.register_read(payload_ptr, len);
                let write_err_len = ctx.register_write_as(err_ptr);

                let payload = ctx.read(read_payload)?;

                let state = ctx.host_state_mut();
                let res = state.ext.reply_push(&payload).into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

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
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let state = ctx.host_state_mut();
                let mut f = || {
                    state.ext.reply_push_input(offset, len)?;
                    state
                        .ext
                        .reply_commit(ReplyPacket::new(Default::default(), value), delay)
                };
                let res = f().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_len = ctx.register_write_as(err_ptr);

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .reply_push_input(offset, len)
                    .into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

                ctx.write_as(write_err_len, len.to_le_bytes())
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
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

                let value = if value_ptr != PTR_SPECIAL {
                    let read_value = ctx.register_read_decoded(value_ptr);
                    ctx.read_decoded(read_value)?
                } else {
                    0
                };

                let state = ctx.host_state_mut();
                let mut f = || {
                    state.ext.reply_push_input(offset, len)?;
                    state.ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                };
                let res = f().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_pid_value = ctx.register_read_as(pid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

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
                let res = f().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
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
                let write_err_len = ctx.register_write_as(err_ptr);

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .send_push_input(handle, offset, len)
                    .into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

                ctx.write_as(write_err_len, len.to_le_bytes())
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
                let read_pid_value = ctx.register_read_as(pid_value_ptr);
                let write_err_mid = ctx.register_write_as(err_mid_ptr);

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
                let res = f().into_ext_error(state)?;
                ctx.write_as(write_err_mid, LengthWithHash::from(res))?;
                Ok(())
            })
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, string_ptr: u32, len: u32| -> EmptyOutput {
                let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

                ctx.run(|ctx| {
                    let read_data = ctx.register_read(string_ptr, len);

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
                let write_err_rid = ctx.register_write_as(err_rid_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.reserve_gas(gas, duration).into_ext_error(state)?;
                ctx.write_as(write_err_rid, LengthWithHash::from(res))?;
                Ok(())
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
                let read_reservation_id = ctx.register_read_decoded(reservation_id_ptr);
                let write_err_unreserved = ctx.register_write_as(err_unreserved_ptr);

                let id = ctx.read_decoded(read_reservation_id)?;

                let state = ctx.host_state_mut();
                let res = state.ext.unreserve_gas(id).into_ext_error(state)?;
                ctx.write_as(write_err_unreserved, LengthWithGas::from(res))?;
                Ok(())
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
                let write_err_len = ctx.register_write_as(err_ptr);

                let state = ctx.host_state_mut();
                let res = state.ext.system_reserve_gas(gas).into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

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
                let write_gas = ctx.register_write_as(gas_ptr);

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
                let write_message_id = ctx.register_write_as(message_id_ptr);

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
                let write_program_id = ctx.register_write_as(program_id_ptr);

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
                let write_source = ctx.register_write_as(source_ptr);

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
                let write_value = ctx.register_write_as(value_ptr);

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
                let write_value = ctx.register_write_as(value_ptr);

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
                let read_subject = ctx.register_read_decoded(subject_ptr);
                let write_bn_random = ctx.register_write_as::<BlockNumberWithHash>(bn_random_ptr);

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
                let read_message_id = ctx.register_read_decoded(message_id_ptr);
                let write_err_len = ctx.register_write_as(err_ptr);

                let message_id = ctx.read_decoded(read_message_id)?;

                let state = ctx.host_state_mut();
                let res = state.ext.wake(message_id, delay).into_ext_error(state)?;
                let len = res.err().unwrap_or(0);

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

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .create_program(InitPacket::new(code_id.into(), salt, payload, value), delay)
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res))?;
                Ok(())
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

                let state = ctx.host_state_mut();
                let res = state
                    .ext
                    .create_program(
                        InitPacket::new_with_gas(code_id.into(), salt, payload, gas_limit, value),
                        delay,
                    )
                    .into_ext_error(state)?;
                ctx.write_as(write_err_mid_pid, LengthWithTwoHashes::from(res))?;
                Ok(())
            })
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         err_buf_ptr: u32,
                         err_len_ptr: u32|
              -> EmptyOutput {
            let mut ctx = CallerWrap::prepare(caller, forbidden, memory)?;

            ctx.run(|ctx| {
                let state = ctx.host_state_mut();
                let last_err = match state.err.clone() {
                    FuncError::Core(maybe_ext) => maybe_ext
                        .into_ext_error()
                        .map_err(|_| ExtError::SyscallUsage),
                    _ => Err(ExtError::SyscallUsage),
                };

                let write_err_len = ctx.register_write_as(err_len_ptr);
                let len: u32 = match last_err {
                    Ok(err) => {
                        let err = err.encode();
                        let write_error_bytes = ctx.register_write(err_buf_ptr, err.len() as u32);
                        ctx.write(write_error_bytes, err.as_ref())?;
                        0
                    }
                    Err(err) => err.encoded_size() as u32,
                };

                ctx.host_state_mut()
                    .ext
                    .charge_error()
                    .map_err(FuncError::Core)?;
                ctx.write_as(write_err_len, len.to_le_bytes())?;
                Ok(())
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

            host_state.err = FuncError::Core(host_state.ext.out_of_allowance());

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }
}
