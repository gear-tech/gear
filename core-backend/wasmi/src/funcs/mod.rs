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
    fmt::{self, Debug},
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
    memory::Memory,
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
};
use gear_core_errors::{CoreError, MemoryError};
use wasmi::{
    core::{Trap, TrapCode},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
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

macro_rules! host_state_mut {
    ($caller:ident) => {
        $caller
            .host_data_mut()
            .as_mut()
            .expect("host_state should be set before execution")
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

macro_rules! exit_if {
    ($forbidden:ident, $caller:ident) => {
        if $forbidden {
            host_state_mut!($caller).err = FuncError::Core(E::Error::forbidden_function());
            return Err(TrapCode::Unreachable.into());
        }
    };
}

#[derive(Debug, derive_more::Display, Encode, Decode)]
pub enum FuncError<E> {
    #[display(fmt = "{}", _0)]
    Core(E),
    #[display(fmt = "Runtime Error")]
    HostError,
    #[display(fmt = "{}", _0)]
    Memory(MemoryError),
    #[display(fmt = "{}", _0)]
    RuntimeBufferSize(RuntimeBufferSizeError),
    #[display(fmt = "{}", _0)]
    PayloadBufferSize(PayloadSizeError),
    #[display(fmt = "Exit code ran into non-reply scenario")]
    NonReplyExitCode,
    #[display(fmt = "Not running in reply context")]
    NoReplyContext,
    #[display(fmt = "Failed to parse debug string")]
    DebugStringParsing,
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {:?}", _0)]
    Terminated(TerminationReason),
    #[display(
        fmt = "Cannot take data by indexes {:?} from message with size {}",
        _0,
        _1
    )]
    ReadWrongRange(Range<u32>, u32),
    #[display(fmt = "Overflow at {} + len {} in `gr_read`", _0, _1)]
    ReadLenOverflow(u32, u32),
}

impl<E> FuncError<E>
where
    E: fmt::Display,
{
    fn as_core(&self) -> Option<&E> {
        match self {
            Self::Core(err) => Some(err),
            _ => None,
        }
    }

    pub fn into_termination_reason(self) -> TerminationReason {
        match self {
            Self::Terminated(reason) => reason,
            err => TerminationReason::Trap(TrapExplanation::Other(err.to_string().into())),
        }
    }
}

impl<E> From<MemoryError> for FuncError<E> {
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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
                    return Err(TrapCode::Unreachable.into());
                }
            };

            if last_idx > message.len() as u32 {
                let err = FuncError::ReadWrongRange(at..last_idx, message.len() as u32);
                let size = Encode::encoded_size(&err) as u32;
                host_state.err = err;
                return Ok((size,));
            }

            let buffer = message[at as usize..last_idx as usize].to_vec();
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let size = host_state.ext.size();
            match size {
                Ok(size) => match u32::try_from(size) {
                    Ok(size) => Ok((size,)),
                    Err(_) => {
                        host_state.err = FuncError::HostError;
                        Err(TrapCode::Unreachable.into())
                    }
                },
                Err(e) => {
                    host_state.err = FuncError::Core(e);
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
            exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, inheritor_id_ptr)
            };

            host_state_mut!(caller).err = match read_result {
                Ok(id) => FuncError::Terminated(TerminationReason::Exit(id)),
                Err(e) => e.into(),
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let exit_code = match host_state.ext.exit_code() {
                Ok(c) => c,
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state.err = err;
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

    pub fn gas(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, gas: u32| -> EmptyOutput {
            let host_state = host_state_mut!(caller);

            host_state
                .ext
                .gas(gas)
                .map_err(FuncError::Core)
                .map_err(|e| {
                    if let Some(TerminationReason::GasAllowanceExceeded) = e
                        .as_core()
                        .and_then(AsTerminationReason::as_termination_reason)
                    {
                        host_state.err =
                            FuncError::Terminated(TerminationReason::GasAllowanceExceeded);
                    }

                    TrapCode::Unreachable.into()
                })
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
                exit_if!(forbidden, caller);

                let mut host_state = caller.host_data_mut().take();

                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                let page = host_state
                    .as_mut()
                    .expect("alloc; should be set")
                    .ext
                    .alloc(pages.into(), &mut memory_wrap);

                *caller.host_data_mut() = host_state;

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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            if let Err(e) = host_state.ext.free(page.into()).map_err(FuncError::Core) {
                log::debug!("FREE ERROR: {e}");
                host_state.err = e;
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            match host_state.ext.block_height() {
                Ok(h) => Ok((h,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn block_timestamp(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u64> {
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            match host_state.ext.block_timestamp() {
                Ok(t) => Ok((t,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, origin_ptr: u32| -> EmptyOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let origin = match host_state.ext.origin() {
                    Ok(o) => o,
                    Err(e) => {
                        host_state.err = FuncError::Core(e);
                        return Err(TrapCode::Unreachable.into());
                    }
                };

                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                match memory_wrap.write(origin_ptr as usize, origin.as_ref()) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        host_state_mut!(caller).err = e.into();

                        Err(TrapCode::Unreachable.into())
                    }
                }
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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let call_result = host_state.ext.reply_to();
            let message_id = match call_result {
                Ok(m) => m,
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state.err = err;
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
            exit_if!(forbidden, caller);

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
            exit_if!(forbidden, caller);

            let mut buffer = RuntimeBuffer::try_new_default(string_len as usize).map_err(|e| {
                host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(string_ptr as usize, buffer.get_mut())
            };

            let host_state = host_state_mut!(caller);

            process_read_result!(read_result, caller);

            let debug_string = match String::from_utf8(buffer.into_vec()) {
                Ok(s) => s,
                Err(_e) => {
                    host_state.err = FuncError::DebugStringParsing;

                    return Err(TrapCode::Unreachable.into());
                }
            };

            let debug_result = host_state.ext.debug(&debug_string);

            match debug_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state.err = FuncError::Core(e);

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> FnResult<u64> {
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            match host_state.ext.gas_available() {
                Ok(gas) => Ok((gas,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let message_id = match host_state.ext.message_id() {
                Ok(o) => o,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(TrapCode::Unreachable.into());
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(message_id_ptr as usize, message_id.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let program_id = match host_state.ext.program_id() {
                Ok(pid) => pid,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(TrapCode::Unreachable.into());
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(program_id_ptr as usize, program_id.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, source_ptr: u32| -> EmptyOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let source = host_state.ext.source().map_err(|e| {
                    host_state.err = FuncError::Core(e);

                    Trap::from(TrapCode::Unreachable)
                })?;

                let write_result = {
                    let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                    memory_wrap.write(source_ptr as usize, &source.encode())
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

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let value = host_state.ext.value().map_err(|e| {
                    host_state.err = FuncError::Core(e);

                    Trap::from(TrapCode::Unreachable)
                })?;

                let write_result = {
                    let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                    memory_wrap.write(value_ptr as usize, &value.encode())
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

    pub fn value_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let value_available = host_state.ext.value_available().map_err(|e| {
                    host_state.err = FuncError::Core(e);

                    Trap::from(TrapCode::Unreachable)
                })?;

                let write_result = {
                    let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                    memory_wrap.write(value_ptr as usize, &value_available.encode())
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

    pub fn random(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         subject_ptr: u32,
                         subject_len: u32,
                         random_ptr: u32,
                         bn_ptr: u32|
              -> EmptyOutput {
            exit_if!(forbidden, caller);

            let mut subject = vec![0; subject_len as usize];

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(subject_ptr as usize, subject.as_mut())
            };

            process_read_result!(read_result, caller);

            subject.reserve(32);

            let host_state = caller
                .host_data()
                .as_ref()
                .expect("host_state should be set before execution");

            let (random, random_bn) = host_state.ext.random();
            subject.extend_from_slice(random);

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .write(random_ptr as usize, blake2b(32, &[], &subject).as_bytes())
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
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            host_state.err = match host_state.ext.leave() {
                Ok(_) => FuncError::Terminated(TerminationReason::Leave),
                Err(e) => FuncError::Core(e),
            };

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> EmptyOutput {
            exit_if!(forbidden, caller);

            let host_state = host_state_mut!(caller);
            let err = host_state
                .ext
                .wait()
                .map_err(FuncError::Core)
                .err()
                .unwrap_or_else(|| {
                    FuncError::Terminated(TerminationReason::Wait(None, MessageWaitedType::Wait))
                });
            host_state.err = err;

            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let call_result = host_state.ext.wait_for(duration);

                host_state.err = match call_result {
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
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let call_result = host_state.ext.wait_up_to(duration);

                host_state.err = match call_result {
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
            exit_if!(forbidden, caller);

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, message_id_ptr)
            };

            let message_id = process_read_result!(read_result, caller);

            let host_state = host_state_mut!(caller);

            match host_state.ext.wake(message_id, delay) {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    let err = FuncError::Core(e);
                    let size = Encode::encoded_size(&err) as u32;
                    host_state.err = err;
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
            exit_if!(forbidden, caller);

            let mut salt = vec![0; salt_len as usize]; // Consider using here `LimitedVec`.

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

            let host_state = host_state_mut!(caller);

            let call_result = host_state
                .ext
                .create_program(InitPacket::new(code_id, salt, payload, value), delay);

            let (message_id, program_id) = match call_result {
                Ok(r) => r,
                Err(e) => match e.into_ext_error() {
                    Ok(ext_error) => {
                        return Ok((ext_error.encoded_size() as u32,));
                    }
                    Err(e) => {
                        host_state.err = FuncError::Core(e);
                        return Err(TrapCode::Unreachable.into());
                    }
                },
            };

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);

                memory_wrap
                    .write(message_id_ptr as usize, message_id.as_ref())
                    .and_then(|_| memory_wrap.write(program_id_ptr as usize, program_id.as_ref()))
            };

            match write_result {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    // this is safe since we own the caller, don't change its host_data
                    // and checked for absence before
                    caller
                        .host_data_mut()
                        .as_mut()
                        .expect("host_data untouched")
                        .err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
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
            exit_if!(forbidden, caller);

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

            let host_state = host_state_mut!(caller);

            let call_result = host_state.ext.create_program(
                InitPacket::new_with_gas(code_id, salt, payload, gas_limit, value),
                delay,
            );

            let (message_id, program_id) = match call_result {
                Ok(r) => r,
                Err(e) => match e.into_ext_error() {
                    Ok(ext_error) => {
                        return Ok((ext_error.encoded_size() as u32,));
                    }
                    Err(e) => {
                        host_state.err = FuncError::Core(e);
                        return Err(TrapCode::Unreachable.into());
                    }
                },
            };

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);

                memory_wrap
                    .write(message_id_ptr as usize, message_id.as_ref())
                    .and_then(|_| memory_wrap.write(program_id_ptr as usize, program_id.as_ref()))
            };

            match write_result {
                Ok(_) => Ok((0,)),
                Err(e) => {
                    // this is safe since we own the caller, don't change its host_data
                    // and checked for absence before
                    caller
                        .host_data_mut()
                        .as_mut()
                        .expect("host_data untouched")
                        .err = e.into();

                    Err(TrapCode::Unreachable.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, buffer_ptr: u32| -> FallibleOutput {
                exit_if!(forbidden, caller);

                let host_state = host_state_mut!(caller);
                let error = match host_state.ext.last_error() {
                    Ok(e) => e,
                    Err(e) => {
                        let err = FuncError::Core(e);
                        let size = Encode::encoded_size(&err) as u32;
                        host_state.err = err;
                        return Ok((size,));
                    }
                };

                let encoded = error.encode();
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
}
