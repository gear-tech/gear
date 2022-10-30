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
    memory::{read_memory_as, MemoryWrapRef},
    state::HostState,
};
#[cfg(not(feature = "std"))]
use alloc::string::ToString;
use alloc::{string::String, vec};
use blake2_rfc::blake2b::blake2b;
use codec::{Decode, Encode};
use core::{
    convert::TryFrom,
    fmt::{Debug, Display},
    marker::PhantomData,
    ops::Range,
};
use gear_backend_common::{
    error_processor::IntoExtError, AsTerminationReason, IntoExtInfo, TerminationReason,
    TrapExplanation,
};
use gear_core::{
    buffer::{RuntimeBuffer, RuntimeBufferSizeError},
    env::Ext,
    ids::ReservationId,
    memory::Memory,
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
};
use gear_core_errors::{CoreError, MemoryError};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use internal::host_state_mut;
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Extern, Func, Memory as WasmiMemory, Store,
};

fn get_caller_memory<'a, E: Ext + IntoExtInfo<E::Error> + 'static>(
    caller: &'a mut Caller<'_, HostState<E>>,
    mem: &WasmiMemory,
) -> MemoryWrapRef<'a, E> {
    let store = caller.as_context_mut();
    MemoryWrapRef {
        memory: *mem,
        store,
    }
}

macro_rules! process_infalliable_call {
    ($caller:ident, $memory:ident, $call:expr, $write:expr) => {
        internal::process_infalliable_call($caller, $memory, $call, $write).map_err(|e| match e {
            internal::Error::HostStateNone => {
                unreachable!("host_state should be set before execution")
            }
            internal::Error::Trap(t) => t,
        })
    };
}

macro_rules! process_call_unit_result {
    ($caller:ident, $call:expr) => {
        internal::process_call_unit_result($caller, $call).map_err(|e| match e {
            internal::Error::HostStateNone => {
                unreachable!("host_state should be set before execution")
            }
            internal::Error::Trap(t) => t,
        })
    };
}

macro_rules! process_call_result {
    ($caller:ident, $memory:ident, $call:expr, $write:expr) => {
        internal::process_call_result($caller, $memory, $call, $write).map_err(|e| match e {
            internal::Error::HostStateNone => {
                unreachable!("host_state should be set before execution")
            }
            internal::Error::Trap(t) => t,
        })
    };
}

macro_rules! process_call_result_as_ref {
    ($caller:ident, $memory:ident, $call:expr, $offset:ident) => {
        internal::process_call_result_as_ref($caller, $memory, $call, $offset).map_err(
            |e| match e {
                internal::Error::HostStateNone => {
                    unreachable!("host_state should be set before execution")
                }
                internal::Error::Trap(t) => t,
            },
        )
    };
}

macro_rules! process_read_result {
    ($read_result:ident, $caller:ident) => {
        match $read_result {
            Ok(value) => value,
            Err(e) => {
                host_state_mut!($caller).err = e.into();
                return Err(TrapCode::Unreachable.into());
            }
        }
    };
}

macro_rules! update_or_exit_if {
    ($forbidden:ident, $caller:ident) => {
        if $forbidden {
            host_state_mut!($caller).err = FuncError::Core(E::Error::forbidden_function());
            return Err(TrapCode::Unreachable.into());
        }

        let gas = $caller
            .get_export(GLOBAL_NAME_GAS)
            .and_then(Extern::into_global)
            .and_then(|g| g.get(&$caller).try_into::<i64>())
            .ok_or({
                host_state_mut!($caller).err = FuncError::HostError;
                Trap::from(TrapCode::Unreachable)
            })? as u64;

        let allowance = $caller
            .get_export(GLOBAL_NAME_ALLOWANCE)
            .and_then(Extern::into_global)
            .and_then(|g| g.get(&$caller).try_into::<i64>())
            .ok_or({
                host_state_mut!($caller).err = FuncError::HostError;
                Trap::from(TrapCode::Unreachable)
            })? as u64;

        host_state_mut!($caller).ext.update_counters(gas, allowance);
    };
}

macro_rules! update_globals {
    ($caller:ident) => {
        match internal::update_globals!($caller) {
            Ok(_) => (),
            Err(e) => return Err(e),
        }
    };
}

#[derive(Debug, derive_more::Display, Encode, Decode)]
pub enum FuncError<E: Display> {
    #[display(fmt = "{_0}")]
    Core(E),
    #[display(fmt = "Runtime Error")]
    HostError,
    #[display(fmt = "{_0}")]
    Memory(MemoryError),
    #[display(fmt = "{_0}")]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "{_0}")]
    PayloadBufferSize(PayloadSizeError),
    #[display(fmt = "Exit code ran into non-reply scenario")]
    NonReplyExitCode,
    #[display(fmt = "Not running in reply context")]
    NoReplyContext,
    #[display(fmt = "Failed to parse debug string")]
    DebugStringParsing,
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {_0:?}")]
    Terminated(TerminationReason),
    #[display(
        fmt = "Cannot take data by indexes {:?} from message with size {}",
        _0,
        _1
    )]
    ReadWrongRange(Range<u32>, u32),
    #[display(fmt = "Overflow at {_0} + len {_1} in `gr_read`")]
    ReadLenOverflow(u32, u32),
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
type FallibleOutput = FnResult<u32>;

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: Encode + AsTerminationReason + IntoExtError,
{
    pub fn send(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         destination_ptr: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, destination_ptr)
                    .and_then(|id| read_memory_as(&memory_wrap, value_ptr).map(|value| (id, value)))
                    .and_then(|(id, value)| {
                        memory_wrap
                            .read(payload_ptr as usize, payload.get_mut())
                            .map(|_| (id, value))
                    })
            };

            let (destination, value) = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| ext.send(HandlePacket::new(destination, payload, value), delay),
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         destination_ptr: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, destination_ptr)
                    .and_then(|id| read_memory_as(&memory_wrap, value_ptr).map(|value| (id, value)))
                    .and_then(|(id, value)| {
                        memory_wrap
                            .read(payload_ptr as usize, payload.get_mut())
                            .map(|_| (id, value))
                    })
            };

            let (destination, value) = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send(
                        HandlePacket::new_with_gas(destination, payload, gas_limit, value),
                        delay,
                    )
                },
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         handle: u32,
                         destination_ptr: u32,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, destination_ptr)
                    .and_then(|id| read_memory_as(&memory_wrap, value_ptr).map(|value| (id, value)))
            };

            let (destination, value) = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send_commit(
                        handle,
                        HandlePacket::new(destination, Default::default(), value),
                        delay,
                    )
                },
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_commit_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         handle: u32,
                         destination_ptr: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, destination_ptr)
                    .and_then(|id| read_memory_as(&memory_wrap, value_ptr).map(|value| (id, value)))
            };

            let (destination, value) = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send_commit(
                        handle,
                        HandlePacket::new_with_gas(
                            destination,
                            Default::default(),
                            gas_limit,
                            value,
                        ),
                        delay,
                    )
                },
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_init(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         handle_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            process_call_result!(
                caller,
                memory,
                |ext| ext.send_init(),
                |memory_wrap, handle| memory_wrap.write(handle_ptr as usize, &handle.to_le_bytes())
            )
        };

        Func::wrap(store, func)
    }

    pub fn send_push(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         handle: u32,
                         payload_ptr: u32,
                         payload_len: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(payload_ptr as usize, payload.get_mut())
            };

            process_read_result!(read_result, caller);

            process_call_unit_result!(caller, |ext| ext.send_push(handle, payload.get()))
        };

        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         at: u32,
                         len: u32,
                         buffer_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);

            let last_idx = match at.checked_add(len) {
                Some(i) => i,
                None => {
                    let err = FuncError::ReadLenOverflow(at, len);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state.err = err;
                    return Ok((size,));
                }
            };

            let call_result = host_state.ext.read();
            let message = match call_result {
                Ok(m) => m,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    update_globals!(caller);
                    return Err(TrapCode::Unreachable.into());
                }
            };

            if last_idx > message.len() as u32 {
                let err = FuncError::ReadWrongRange(at..last_idx, message.len() as u32);
                let size = Encode::encoded_size(&err) as u32;
                host_state.err = err;
                update_globals!(caller);
                return Ok((size,));
            }

            let buffer = message[at as usize..last_idx as usize].to_vec();
            update_globals!(caller);
            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(buffer_ptr as usize, &buffer) {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u32> {
            update_or_exit_if!(forbidden, caller);

            let size = host_state_mut!(caller).ext.size();
            update_globals!(caller);

            match size {
                Ok(size) => match u32::try_from(size) {
                    Ok(size) => Ok((size,)),
                    Err(_) => {
                        host_state_mut!(caller).err = FuncError::HostError;
                        Err(TrapCode::Unreachable.into())
                    }
                },
                Err(e) => {
                    host_state_mut!(caller).err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         inheritor_id_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, inheritor_id_ptr)
            };

            let call_result = host_state_mut!(caller).ext.exit();
            update_globals!(caller);

            host_state_mut!(caller).err = match call_result {
                Err(e) => FuncError::Core(e),
                Ok(_) => match read_result {
                    Ok(id) => FuncError::Terminated(TerminationReason::Exit(id)),
                    Err(e) => e.into(),
                },
            };

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn exit_code(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         exit_code_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let exit_code = match host_state.ext.exit_code() {
                Ok(c) => c,
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state.err = err;
                    update_globals!(caller);
                    return Ok((size,));
                }
            };

            process_call_result!(
                caller,
                memory,
                |_ext| Ok(exit_code),
                |memory_wrap, exit_code| memory_wrap
                    .write(exit_code_ptr as usize, &exit_code.to_le_bytes())
            )
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
                update_or_exit_if!(forbidden, caller);

                let mut host_state = caller.host_data_mut().take();

                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                let page = host_state
                    .as_mut()
                    .expect("alloc; should be set")
                    .ext
                    .alloc(pages.into(), &mut memory_wrap);

                *caller.host_data_mut() = host_state;
                update_globals!(caller);

                match page {
                    Ok(page) => {
                        log::debug!("ALLOC PAGES: {} pages at {:?}", pages, page);

                        Ok((page.0,))
                    }
                    Err(e) => {
                        host_state_mut!(caller).err = FuncError::Core(e);

                        Err(TrapCode::Unreachable.into())
                    }
                }
            };

        Func::wrap(store, func)
    }

    pub fn free(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, page: u32| -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.free(page.into());
            update_globals!(caller);

            if let Err(e) = call_result.map_err(FuncError::Core) {
                log::debug!("FREE ERROR: {e}");
                host_state_mut!(caller).err = e;
                Err(TrapCode::Unreachable.into())
            } else {
                log::debug!("FREE: {page}");
                Ok(())
            }
        };

        Func::wrap(store, func)
    }

    pub fn block_height(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u32> {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.block_height();
            update_globals!(caller);

            match call_result {
                Ok(h) => Ok((h,)),
                Err(e) => {
                    host_state_mut!(caller).err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn block_timestamp(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u64> {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.block_timestamp();
            update_globals!(caller);

            match call_result {
                Ok(t) => Ok((t,)),
                Err(e) => {
                    host_state_mut!(caller).err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         origin_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            process_infalliable_call!(caller, memory, |ext| ext.origin(), |memory, origin| memory
                .write(origin_ptr as usize, origin.as_ref()))
        };

        Func::wrap(store, func)
    }

    pub fn reply(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         payload_len: u32,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32| {
            update_or_exit_if!(forbidden, caller);

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                memory_wrap
                    .read(payload_ptr as usize, payload.get_mut())
                    .and_then(|_| read_memory_as(&memory_wrap, value_ptr))
            };

            let value = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| ext.reply(ReplyPacket::new(payload, value), delay),
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         payload_len: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr as usize, payload.get_mut())
                    .and_then(|_| read_memory_as(&memory_wrap, value_ptr))
            };

            let value = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| ext.reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay),
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_commit(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32| {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, value_ptr)
            };

            let value = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| ext.reply_commit(ReplyPacket::new(Default::default(), value), delay),
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_commit_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                read_memory_as(&memory_wrap, value_ptr)
            };

            let value = process_read_result!(read_result, caller);

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                },
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         message_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.reply_to();
            update_globals!(caller);

            let message_id = match call_result {
                Ok(m) => m,
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state_mut!(caller).err = err;
                    return Ok((size,));
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(message_id_ptr as usize, message_id.as_ref()) {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn reply_push(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         payload_ptr: u32,
                         payload_len: u32| {
            update_or_exit_if!(forbidden, caller);

            let mut payload =
                RuntimeBuffer::try_new_default(payload_len as usize).map_err(|e| {
                    host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(payload_ptr as usize, payload.get_mut())
            };

            process_read_result!(read_result, caller);

            process_call_unit_result!(caller, |ext| ext.reply_push(payload.get()))
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         string_ptr: u32,
                         string_len: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let mut buffer = RuntimeBuffer::try_new_default(string_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(string_ptr as usize, buffer.get_mut())
            };

            process_read_result!(read_result, caller);

            let debug_string = match String::from_utf8(buffer.into_vec()) {
                Ok(s) => s,
                Err(_e) => {
                    host_state_mut!(caller).err = FuncError::DebugStringParsing;

                    return Err(TrapCode::Unreachable.into());
                }
            };

            let debug_result = host_state_mut!(caller).ext.debug(&debug_string);
            update_globals!(caller);

            match debug_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = FuncError::Core(e);

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn reserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         gas_amount: u64,
                         duration: u32,
                         id_ptr: u32| {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller)
                .ext
                .reserve_gas(gas_amount, duration);
            update_globals!(caller);

            let id = match call_result {
                Ok(o) => o,
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state_mut!(caller).err = err;
                    return Ok((size,));
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(id_ptr as usize, id.as_ref()) {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn unreserve_gas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, id_ptr: u32, amount_ptr: u32| {
                update_or_exit_if!(forbidden, caller);

                let read_result = {
                    let memory_wrap = get_caller_memory(&mut caller, &memory);
                    read_memory_as::<ReservationId>(&memory_wrap, id_ptr)
                };

                let id = process_read_result!(read_result, caller);

                let call_result = host_state_mut!(caller).ext.unreserve_gas(id);
                update_globals!(caller);

                let gas_amount = match call_result {
                    Ok(gas_amount) => gas_amount,
                    Err(e) => {
                        let err = FuncError::Core(e);
                        let size = Encode::encoded_size(&err) as u32;
                        host_state_mut!(caller).err = err;
                        return Ok((size,));
                    }
                };

                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                if let Err(e) =
                    memory_wrap.write(amount_ptr as usize, gas_amount.to_le_bytes().as_ref())
                {
                    host_state_mut!(caller).err = e.into();
                    return Err(TrapCode::Unreachable.into());
                }

                Ok((0,))
            };

        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u64> {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.gas_available();
            update_globals!(caller);

            match call_result {
                Ok(gas) => Ok((gas,)),
                Err(e) => {
                    host_state_mut!(caller).err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn message_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         message_id_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            process_infalliable_call!(
                caller,
                memory,
                |ext| ext.message_id(),
                |memory, message_id| memory.write(message_id_ptr as usize, message_id.as_ref())
            )
        };

        Func::wrap(store, func)
    }

    pub fn program_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         program_id_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            process_infalliable_call!(
                caller,
                memory,
                |ext| ext.program_id(),
                |memory, program_id| memory.write(program_id_ptr as usize, program_id.as_ref())
            )
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         source_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            process_infalliable_call!(caller, memory, |ext| ext.source(), |memory, source| memory
                .write(source_ptr as usize, &source.encode()))
        };

        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
                update_or_exit_if!(forbidden, caller);

                process_infalliable_call!(caller, memory, |ext| ext.value(), |memory, value| memory
                    .write(value_ptr as usize, &value.encode()))
            };

        Func::wrap(store, func)
    }

    pub fn value_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
                update_or_exit_if!(forbidden, caller);

                process_infalliable_call!(
                    caller,
                    memory,
                    |ext| ext.value_available(),
                    |memory, value_available| memory
                        .write(value_ptr as usize, &value_available.encode())
                )
            };

        Func::wrap(store, func)
    }

    pub fn random(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         subject_ptr: u32,
                         subject_len: u32,
                         random_ptr: u32,
                         bn_ptr: u32|
              -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let mut subject =
                RuntimeBuffer::try_new_default(subject_len as usize).map_err(|e| {
                    host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(subject_ptr as usize, subject.get_mut())
            };

            process_read_result!(read_result, caller);

            let host_state = caller
                .host_data()
                .as_ref()
                .expect("host_state should be set before execution");

            let (random, random_bn) = host_state.ext.random();

            subject.try_extend_from_slice(random).map_err(|e| {
                host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .write(
                        random_ptr as usize,
                        blake2b(32, &[], subject.get()).as_bytes(),
                    )
                    .and_then(|_| memory_wrap.write(bn_ptr as usize, &random_bn.to_le_bytes()))
            };

            match write_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.leave();
            update_globals!(caller);

            host_state_mut!(caller).err = match call_result {
                Ok(_) => FuncError::Terminated(TerminationReason::Leave),
                Err(e) => FuncError::Core(e),
            };

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> EmptyOutput {
            update_or_exit_if!(forbidden, caller);

            let call_result = host_state_mut!(caller).ext.wait();
            update_globals!(caller);

            host_state_mut!(caller).err = match call_result {
                Ok(_) => {
                    FuncError::Terminated(TerminationReason::Wait(None, MessageWaitedType::Wait))
                }
                Err(e) => FuncError::Core(e),
            };

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
                update_or_exit_if!(forbidden, caller);

                let call_result = host_state_mut!(caller).ext.wait_for(duration);
                update_globals!(caller);

                host_state_mut!(caller).err = match call_result {
                    Ok(_) => FuncError::Terminated(TerminationReason::Wait(
                        Some(duration),
                        MessageWaitedType::WaitFor,
                    )),
                    Err(e) => FuncError::Core(e),
                };

                Err(TrapCode::Unreachable.into())
            };

        Func::wrap(store, func)
    }

    pub fn wait_up_to(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
                update_or_exit_if!(forbidden, caller);

                let call_result = host_state_mut!(caller).ext.wait_up_to(duration);
                update_globals!(caller);

                host_state_mut!(caller).err = match call_result {
                    Ok(enough) => FuncError::Terminated(TerminationReason::Wait(
                        Some(duration),
                        if enough {
                            MessageWaitedType::WaitUpToFull
                        } else {
                            MessageWaitedType::WaitUpTo
                        },
                    )),
                    Err(e) => FuncError::Core(e),
                };

                Err(TrapCode::Unreachable.into())
            };

        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         message_id_ptr: u32,
                         delay: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, message_id_ptr)
            };

            let message_id = process_read_result!(read_result, caller);

            let call_result = host_state_mut!(caller).ext.wake(message_id, delay);
            update_globals!(caller);

            match call_result {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state_mut!(caller).err = err;
                    Ok((size,))
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn create_program(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         code_id_ptr: u32,
                         salt_ptr: u32,
                         salt_len: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32,
                         program_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            // Consider using here `LimitedVec`.
            let mut salt = vec![0; salt_len as usize];

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr as usize, payload.get_mut())
                    .and_then(|_| memory_wrap.read(salt_ptr as usize, salt.as_mut()))
                    .and_then(|_| read_memory_as(&memory_wrap, value_ptr))
                    .and_then(|value| {
                        read_memory_as(&memory_wrap, code_id_ptr).map(|code_id| (value, code_id))
                    })
            };

            let (value, code_id) = process_read_result!(read_result, caller);

            process_call_result!(
                caller,
                memory,
                |ext| ext.create_program(InitPacket::new(code_id, salt, payload, value), delay),
                |memory_wrap, (message_id, program_id)| {
                    memory_wrap
                        .write(message_id_ptr as usize, message_id.as_ref())
                        .and_then(|_| {
                            memory_wrap.write(program_id_ptr as usize, program_id.as_ref())
                        })
                }
            )
        };

        Func::wrap(store, func)
    }

    pub fn create_program_wgas(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         code_id_ptr: u32,
                         salt_ptr: u32,
                         salt_len: u32,
                         payload_ptr: u32,
                         payload_len: u32,
                         gas_limit: u64,
                         value_ptr: u32,
                         delay: u32,
                         message_id_ptr: u32,
                         program_id_ptr: u32|
              -> FallibleOutput {
            update_or_exit_if!(forbidden, caller);

            let mut salt = vec![0u8; salt_len as usize];

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);

                memory_wrap
                    .read(payload_ptr as usize, payload.get_mut())
                    .and_then(|_| memory_wrap.read(salt_ptr as usize, &mut salt))
                    .and_then(|_| read_memory_as(&memory_wrap, value_ptr))
                    .and_then(|value| {
                        read_memory_as(&memory_wrap, code_id_ptr).map(|code_id| (code_id, value))
                    })
            };

            let (code_id, value) = process_read_result!(read_result, caller);

            process_call_result!(
                caller,
                memory,
                |ext| ext.create_program(
                    InitPacket::new_with_gas(code_id, salt, payload, gas_limit, value),
                    delay
                ),
                |memory_wrap, (message_id, program_id)| {
                    memory_wrap
                        .write(message_id_ptr as usize, message_id.as_ref())
                        .and_then(|_| {
                            memory_wrap.write(program_id_ptr as usize, program_id.as_ref())
                        })
                }
            )
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, buffer_ptr: u32| -> FallibleOutput {
                update_or_exit_if!(forbidden, caller);

                let call_result = host_state_mut!(caller).ext.last_error();
                let error = match call_result {
                    Ok(e) => e,
                    Err(e) => {
                        let err = FuncError::Core(e);
                        let size = Encode::encoded_size(&err) as u32;
                        host_state_mut!(caller).err = err;
                        update_globals!(caller);
                        return Ok((size,));
                    }
                };

                let encoded = error.encode();

                update_globals!(caller);

                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                match memory_wrap.write(buffer_ptr as usize, &encoded) {
                    Ok(_) => Ok((0,)),
                    Err(e) => {
                        host_state_mut!(caller).err = e.into();

                        Err(TrapCode::Unreachable.into())
                    }
                }
            };

        Func::wrap(store, func)
    }

    pub fn out_of_gas(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> EmptyOutput {
            let host_state = host_state_mut!(caller);
            host_state.err = FuncError::Core(host_state.ext.out_of_gas());

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn out_of_allowance(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> EmptyOutput {
            let host_state = host_state_mut!(caller);
            host_state.ext.out_of_allowance();
            host_state.err = FuncError::Terminated(TerminationReason::GasAllowanceExceeded);

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }
}
