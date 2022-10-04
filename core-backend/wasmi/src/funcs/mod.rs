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
use alloc::{
    string::{FromUtf8Error, String},
    vec,
};
use codec::Encode;
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
    ids::ProgramId,
    memory::Memory,
    message::{HandlePacket, InitPacket, ReplyPacket, PayloadSizeError, Payload},
};
use gear_core_errors::{CoreError, MemoryError};
use wasmi::{
    core::{HostError, Trap},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
};

fn get_caller_memory<'a, E: Ext + IntoExtInfo + 'static>(
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
        $caller.host_data_mut().as_mut().expect(concat!(
            "line ",
            line!(),
            "; host_state should be set before execution"
        ))
    };
}

macro_rules! process_call_unit_result {
    ($caller:ident, $call:expr) => {
        internal::process_call_unit_result($caller, $call).map_err(|e| match e {
            internal::Error::HostStateNone => unreachable!(concat!(
                "line ",
                line!(),
                "; host_state should be set before execution"
            )),
            internal::Error::Trap(t) => t,
        })
    };
}

macro_rules! process_call_result {
    ($caller:ident, $memory:ident, $call:expr, $write:expr) => {
        internal::process_call_result($caller, $memory, $call, $write).map_err(|e| match e {
            internal::Error::HostStateNone => unreachable!(concat!(
                "line ",
                line!(),
                "; host_state should be set before execution"
            )),
            internal::Error::Trap(t) => t,
        })
    };
}

macro_rules! process_call_result_as_ref {
    ($caller:ident, $memory:ident, $call:expr, $offset:ident) => {
        internal::process_call_result_as_ref($caller, $memory, $call, $offset).map_err(
            |e| match e {
                internal::Error::HostStateNone => unreachable!(concat!(
                    "line ",
                    line!(),
                    "; host_state should be set before execution"
                )),
                internal::Error::Trap(t) => t,
            },
        )
    };
}

#[derive(Debug, derive_more::Display)]
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
    #[display(fmt = "Cannot set u128: {}", _0)]
    SetU128(MemoryError),
    #[display(fmt = "Exit code ran into non-reply scenario")]
    NonReplyExitCode,
    #[display(fmt = "Not running in reply context")]
    NoReplyContext,
    #[display(fmt = "Failed to parse debug string: {}", _0)]
    DebugString(FromUtf8Error),
    #[display(fmt = "`gr_error` expects error occurred earlier")]
    SyscallErrorExpected,
    #[display(fmt = "Terminated: {:?}", _0)]
    Terminated(TerminationReason),
    #[display(
        fmt = "Cannot take data by indexes {:?} from message with size {}",
        _0,
        _1
    )]
    ReadWrongRange(Range<usize>, usize),
    #[display(fmt = "Overflow at {} + len {} in `gr_read`", _0, _1)]
    ReadLenOverflow(usize, usize),
}

#[derive(Debug)]
pub struct DummyHostError;

impl fmt::Display for DummyHostError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DummyHostError")
    }
}

impl HostError for DummyHostError {}

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

impl<E> FuncsHandler<E>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: AsTerminationReason + IntoExtError,
{
    pub fn send(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let program_id_ptr = program_id_ptr as u32 as usize;
            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<ProgramId>(&memory_wrap, program_id_ptr)
                    .and_then(|id| read_memory_as::<u128>(&memory_wrap, value_ptr).map(|v| (id, v)))
                    .and_then(|(id, v)| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (id, v, d)))
                    .and_then(|(id, v, d)| {
                        memory_wrap.read(payload_ptr, payload.get_mut()).map(|_| (id, v, d))
                    })
            };

            let (destination, value, delay) = match read_result {
                Ok((id, v, d)) => (id, v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

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
                         program_id_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: u64,
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let program_id_ptr = program_id_ptr as u32 as usize;
            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<ProgramId>(&memory_wrap, program_id_ptr)
                    .and_then(|id| read_memory_as::<u128>(&memory_wrap, value_ptr).map(|v| (id, v)))
                    .and_then(|(id, v)| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (id, v, d)))
                    .and_then(|(id, v, d)| {
                        memory_wrap.read(payload_ptr, payload.get_mut()).map(|_| (id, v, d))
                    })
            };

            let (destination, value, delay) = match read_result {
                Ok((id, v, d)) => (id, v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send(HandlePacket::new_with_gas(
                        destination,
                        payload,
                        gas_limit,
                        value,
                    ), delay)
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
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         value_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let handle_ptr = handle_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let program_id_ptr = program_id_ptr as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<ProgramId>(&memory_wrap, program_id_ptr)
                    .and_then(|id| read_memory_as::<u128>(&memory_wrap, value_ptr).map(|v| (id, v)))
                    .and_then(|(id, v)| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (id, v, d)))
            };

            let (destination, value, delay) = match read_result {
                Ok((id, v, d)) => (id, v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send_commit(
                        handle_ptr,
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
                         handle_ptr: i32,
                         message_id_ptr: i32,
                         program_id_ptr: i32,
                         gas_limit: u64,
                         value_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let handle_ptr = handle_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let program_id_ptr = program_id_ptr as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<ProgramId>(&memory_wrap, program_id_ptr)
                    .and_then(|id| read_memory_as::<u128>(&memory_wrap, value_ptr).map(|v| (id, v)))
                    .and_then(|(id, v)| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (id, v, d)))
            };

            let (destination, value, delay) = match read_result {
                Ok((id, v, d)) => (id, v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.send_commit(
                        handle_ptr,
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
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, handle_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let handle_ptr = handle_ptr as u32 as usize;

            process_call_result!(
                caller,
                memory,
                |ext| ext.send_init(),
                |memory_wrap, handle| memory_wrap.write(handle_ptr, &handle.to_le_bytes())
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
                         handle_ptr: i32,
                         payload_ptr: i32,
                         payload_len: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let handle_ptr = handle_ptr as u32 as usize;
            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;

            let mut payload = vec![0u8; payload_len];
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(payload_ptr, &mut payload)
            };

            match read_result {
                Ok(_) => (),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    return Err(DummyHostError.into());
                }
            };

            process_call_unit_result!(caller, |ext| ext.send_push(handle_ptr, &payload))
        };

        Func::wrap(store, func)
    }

    pub fn read(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         at: i32,
                         len: i32,
                         destination_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let at = at as u32 as usize;
            let len = len as u32 as usize;
            let destination_ptr = destination_ptr as u32 as usize;

            let last_idx = match at.checked_add(len) {
                Some(i) => i,
                None => {
                    host_state.err = FuncError::ReadLenOverflow(at, len);
                    return Err(DummyHostError.into());
                }
            };

            let call_result = host_state.ext.read();
            let message = match call_result {
                Ok(m) => m,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            if last_idx > message.len() {
                host_state.err = FuncError::ReadWrongRange(at..last_idx, message.len());
                return Err(DummyHostError.into());
            }

            let buffer = message[at..last_idx].to_vec();
            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(destination_ptr, &buffer) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let size = host_state.ext.size();
            match size {
                Ok(size) => match u32::try_from(size) {
                    Ok(size) => Ok((size,)),
                    Err(_) => {
                        host_state.err = FuncError::HostError;
                        Err(DummyHostError.into())
                    }
                },
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         value_dest_ptr: i32|
              -> Result<(), Trap> {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let value_dest_ptr = value_dest_ptr as u32 as usize;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<ProgramId>(&memory_wrap, value_dest_ptr)
            };

            host_state_mut!(caller).err = match read_result {
                Ok(pid) => FuncError::Terminated(TerminationReason::Exit(pid)),
                Err(e) => e.into(),
            };

            Err(DummyHostError.into())
        };

        Func::wrap(store, func)
    }

    pub fn exit_code(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let exit_code = match host_state.ext.exit_code() {
                Ok(c) => c,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            if let Some(exit_code) = exit_code {
                Ok((exit_code,))
            } else {
                host_state.err = FuncError::NonReplyExitCode;
                Err(DummyHostError.into())
            }
        };

        Func::wrap(store, func)
    }

    pub fn gas(store: &mut Store<HostState<E>>) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, val: u32| {
            let host_state = host_state_mut!(caller);

            host_state
                .ext
                .gas(val)
                .map_err(FuncError::Core)
                .map_err(|e| {
                    if let Some(TerminationReason::GasAllowanceExceeded) = e
                        .as_core()
                        .and_then(AsTerminationReason::as_termination_reason)
                    {
                        host_state.err =
                            FuncError::Terminated(TerminationReason::GasAllowanceExceeded);
                    }

                    DummyHostError.into()
                })
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         pages: u32|
              -> Result<(u32,), Trap> {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

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

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn free(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, page: u32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            if let Err(e) = host_state.ext.free(page.into()).map_err(FuncError::Core) {
                log::debug!("FREE ERROR: {e}");
                host_state.err = e;
                Err(DummyHostError.into())
            } else {
                log::debug!("FREE: {page}");
                Ok(())
            }
        };

        Func::wrap(store, func)
    }

    pub fn block_height(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            match host_state.ext.block_height() {
                Ok(h) => Ok((h,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn block_timestamp(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            match host_state.ext.block_timestamp() {
                Ok(t) => Ok((t,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, origin_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let origin_ptr = origin_ptr as u32 as usize;

            let origin = match host_state.ext.origin() {
                Ok(o) => o,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(origin_ptr, origin.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn reply(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr, payload.get_mut())
                    .and_then(|_| read_memory_as::<u128>(&memory_wrap, value_ptr))
                    .and_then(|v| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, d)))
            };

            let (value, delay) = match read_result {
                Ok((v, d)) => (v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

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
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: u64,
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr, payload.get_mut())
                    .and_then(|_| read_memory_as::<u128>(&memory_wrap, value_ptr))
                    .and_then(|v| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, d)))
            };

            let (value, delay) = match read_result {
                Ok((v, d)) => (v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

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
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<u128>(&memory_wrap, value_ptr)
                    .and_then(|v| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, d)))
            };

            let (value, delay) = match read_result {
                Ok((v, d)) => (v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

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
                         value_ptr: i32,
                         message_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let value_ptr = value_ptr as u32 as usize;
            let message_id_ptr = message_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<u128>(&memory_wrap, value_ptr)
                    .and_then(|v| read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, d)))
            };

            let (value, delay) = match read_result {
                Ok((v, d)) => (v, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.reply_commit(ReplyPacket::new_with_gas(
                        Default::default(),
                        gas_limit,
                        value,
                    ),
                    delay)
                },
                message_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, destination_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let destination_ptr = destination_ptr as u32 as usize;

            let call_result = host_state.ext.reply_to();

            let message_id = match call_result {
                Ok(m) => m,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            let message_id = match message_id {
                None => {
                    host_state.err = FuncError::NoReplyContext;
                    return Err(DummyHostError.into());
                }
                Some(m) => m,
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(destination_ptr, message_id.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
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
                         payload_ptr: i32,
                         payload_len: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;

            let mut payload = vec![0u8; payload_len];
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.read(payload_ptr, &mut payload)
            };

            match read_result {
                Ok(_) => (),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();
                    return Err(DummyHostError.into());
                }
            }

            process_call_unit_result!(caller, |ext| ext.reply_push(&payload))
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |mut caller: wasmi::Caller<'_, HostState<E>>, string_ptr: i32, string_len: i32| {
                if forbidden {
                    host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                    return Err(DummyHostError.into());
                }

                let string_ptr = string_ptr as u32 as usize;
                let string_len = string_len as u32 as usize;

                let mut buffer = RuntimeBuffer::try_new_default(string_len)
                    .map_err(|e| {
                        host_state_mut!(caller).err = FuncError::RuntimeBufferSize(e);
                        Trap::from(DummyHostError)
                    })?;
                let read_result = {
                    let memory_wrap = get_caller_memory(&mut caller, &memory);
                    memory_wrap.read(string_ptr, buffer.get_mut())
                };

                let host_state = host_state_mut!(caller);

                match read_result {
                    Ok(_) => (),
                    Err(e) => {
                        host_state.err = FuncError::Memory(e);

                        return Err(DummyHostError.into());
                    }
                };

                let debug_string = match String::from_utf8(buffer.into_vec()) {
                    Ok(s) => s,
                    Err(e) => {
                        host_state.err = FuncError::DebugString(e);

                        return Err(DummyHostError.into());
                    }
                };

                let debug_result = host_state.ext.debug(&debug_string);

                match debug_result {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        host_state.err = FuncError::Core(e);

                        Err(DummyHostError.into())
                    }
                }
            };

        Func::wrap(store, func)
    }

    pub fn gas_available(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            match host_state.ext.gas_available() {
                Ok(g) => Ok((g as i64,)),
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn msg_id(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, msg_id_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let msg_id_ptr = msg_id_ptr as u32 as usize;

            let message_id = match host_state.ext.message_id() {
                Ok(o) => o,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(msg_id_ptr, message_id.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
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
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, program_id_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let program_id_ptr = program_id_ptr as u32 as usize;

            let program_id = match host_state.ext.program_id() {
                Ok(pid) => pid,
                Err(e) => {
                    host_state.err = FuncError::Core(e);
                    return Err(DummyHostError.into());
                }
            };

            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(program_id_ptr, program_id.as_ref()) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, source_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let source_ptr = source_ptr as u32 as usize;

            let source = host_state.ext.source().map_err(|e| {
                host_state.err = FuncError::Core(e);

                Trap::from(DummyHostError)
            })?;

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.write(source_ptr, &source.encode())
            };

            match write_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let value_ptr = value_ptr as u32 as usize;

            let value = host_state.ext.value().map_err(|e| {
                host_state.err = FuncError::Core(e);

                Trap::from(DummyHostError)
            })?;

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.write(value_ptr, &value.encode())
            };

            match write_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
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
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, value_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let value_ptr = value_ptr as u32 as usize;

            let value_available = host_state.ext.value_available().map_err(|e| {
                host_state.err = FuncError::Core(e);

                Trap::from(DummyHostError)
            })?;

            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap.write(value_ptr, &value_available.encode())
            };

            match write_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> Result<(), Trap> {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            host_state.err = match host_state.ext.leave() {
                Ok(_) => FuncError::Terminated(TerminationReason::Leave),
                Err(e) => FuncError::Core(e),
            };

            Err(DummyHostError.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>| -> Result<(), Trap> {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let err = host_state
                .ext
                .wait()
                .map_err(FuncError::Core)
                .err()
                .unwrap_or(FuncError::Terminated(TerminationReason::Wait(None)));
            host_state.err = err;

            Err(DummyHostError.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         duration_ptr: i32|
              -> Result<(), Trap> {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let duration_ptr = duration_ptr as u32 as usize;

            let read_result: Result<u32, _> = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, duration_ptr)
            };

            let host_state = host_state_mut!(caller);

            let duration = match read_result {
                Ok(d) => d,
                Err(e) => {
                    host_state.err = FuncError::Memory(e);

                    return Err(DummyHostError.into());
                }
            };

            let call_result = host_state.ext.wait_for(duration);

            host_state.err = match call_result {
                Ok(_) => FuncError::Terminated(TerminationReason::Wait(Some(duration))),
                Err(e) => FuncError::Core(e),
            };

            Err(DummyHostError.into())
        };

        Func::wrap(store, func)
    }

    pub fn wait_up_to(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>,
                         duration_ptr: i32|
              -> Result<(), Trap> {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let duration_ptr = duration_ptr as u32 as usize;

            let read_result: Result<u32, _> = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as(&memory_wrap, duration_ptr)
            };

            let host_state = host_state_mut!(caller);

            let duration = match read_result {
                Ok(d) => d,
                Err(e) => {
                    host_state.err = FuncError::Memory(e);

                    return Err(DummyHostError.into());
                }
            };

            let call_result = host_state.ext.wait_up_to(duration);

            host_state.err = match call_result {
                Ok(_) => FuncError::Terminated(TerminationReason::Wait(Some(duration))),
                Err(e) => FuncError::Core(e),
            };

            Err(DummyHostError.into())
        };

        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, waker_id_ptr: i32,
        delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                read_memory_as::<[u8; 32]>(&memory_wrap, waker_id_ptr as usize)
                    .and_then(|a| read_memory_as::<u32>(&memory_wrap, delay_ptr as usize).map(|d| (a, d)))
            };

            let (waker_id, delay) = match read_result {
                Ok((a, d)) => (a.into(), d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    return Err(DummyHostError.into());
                }
            };

            let host_state = host_state_mut!(caller);

            match host_state.ext.wake(waker_id, delay) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state.err = FuncError::Core(e);

                    Err(DummyHostError.into())
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
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         value_ptr: i32,
                         program_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let code_hash_ptr = code_hash_ptr as u32 as usize;
            let salt_ptr = salt_ptr as u32 as usize;
            let salt_len = salt_len as u32 as usize;
            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let program_id_ptr = program_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut salt = vec![0u8; salt_len];
            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr, payload.get_mut())
                    .and_then(|_| memory_wrap.read(salt_ptr, &mut salt))
                    .and_then(|_| read_memory_as::<u128>(&memory_wrap, value_ptr))
                    .and_then(|v| {
                        read_memory_as::<[u8; 32]>(&memory_wrap, code_hash_ptr).map(|c| (v, c))
                    })
                    .and_then(|(v, c)| {
                        read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, c, d))
                    })
            };

            let (value, code_hash, delay) = match read_result {
                Ok((v, c, d)) => (v, c, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| ext.create_program(InitPacket::new(code_hash.into(), salt, payload, value), delay),
                program_id_ptr
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
                         code_hash_ptr: i32,
                         salt_ptr: i32,
                         salt_len: i32,
                         payload_ptr: i32,
                         payload_len: i32,
                         gas_limit: u64,
                         value_ptr: i32,
                         program_id_ptr: i32,
                         delay_ptr: i32| {
            if forbidden {
                host_state_mut!(caller).err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let code_hash_ptr = code_hash_ptr as u32 as usize;
            let salt_ptr = salt_ptr as u32 as usize;
            let salt_len = salt_len as u32 as usize;
            let payload_ptr = payload_ptr as u32 as usize;
            let payload_len = payload_len as u32 as usize;
            let value_ptr = value_ptr as u32 as usize;
            let program_id_ptr = program_id_ptr as u32 as usize;
            let delay_ptr = delay_ptr as u32 as usize;

            let mut salt = vec![0u8; salt_len];
            let mut payload = Payload::try_new_default(payload_len)
                .map_err(|e| {
                    host_state_mut!(caller).err = FuncError::PayloadBufferSize(e);
                    Trap::from(DummyHostError)
                })?;
            let read_result = {
                let memory_wrap = get_caller_memory(&mut caller, &memory);
                memory_wrap
                    .read(payload_ptr, payload.get_mut())
                    .and_then(|_| memory_wrap.read(salt_ptr, &mut salt))
                    .and_then(|_| read_memory_as::<u128>(&memory_wrap, value_ptr))
                    .and_then(|v| {
                        read_memory_as::<[u8; 32]>(&memory_wrap, code_hash_ptr).map(|c| (v, c))
                    })
                    .and_then(|(v, c)| {
                        read_memory_as::<u32>(&memory_wrap, delay_ptr).map(|d| (v, c, d))
                    })
            };

            let (value, code_hash, delay) = match read_result {
                Ok((v, c, d)) => (v, c, d),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    return Err(DummyHostError.into());
                }
            };

            process_call_result_as_ref!(
                caller,
                memory,
                |ext| {
                    ext.create_program(InitPacket::new_with_gas(
                        code_hash.into(),
                        salt,
                        payload,
                        gas_limit,
                        value,
                    ), delay)
                },
                program_id_ptr
            )
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |mut caller: wasmi::Caller<'_, HostState<E>>, data_ptr: i32| {
            let host_state = host_state_mut!(caller);
            if forbidden {
                host_state.err = FuncError::Core(E::Error::forbidden_function());
                return Err(DummyHostError.into());
            }

            let data_ptr = data_ptr as u32 as usize;

            let error = match host_state.ext.last_error() {
                Some(e) => e,
                None => {
                    host_state.err = FuncError::SyscallErrorExpected;
                    return Err(DummyHostError.into());
                }
            };

            let encoded = error.encode();
            let mut memory_wrap = get_caller_memory(&mut caller, &memory);
            match memory_wrap.write(data_ptr, &encoded) {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(DummyHostError.into())
                }
            }
        };

        Func::wrap(store, func)
    }
}
