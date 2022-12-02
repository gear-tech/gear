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
    memory::{Memory, PageU32Size, WasmPageNumber},
    message::{
        HandlePacket, InitPacket, MessageWaitedType, Payload, PayloadSizeError, ReplyPacket,
    },
};
use gear_core_errors::{CoreError, MemoryError};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use gsys::{
    BlockNumberWithHash, HashWithValue, LengthWithCode, LengthWithGas, LengthWithHandle,
    LengthWithHash, LengthWithTwoHashes, TwoHashesWithValue,
};
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Func, Memory as WasmiMemory, Store,
};

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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.send(HandlePacket::new(destination, payload, value), delay),
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.send(
                        HandlePacket::new_with_gas(destination, payload, gas_limit, value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.send_commit(
                        handle,
                        HandlePacket::new(destination, Default::default(), value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
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
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.send_init(),
                |res, mut mem_ref| {
                    let err_handle = res
                        .map(|handle| LengthWithHandle {
                            handle,
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHandle {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_handle_ptr, err_handle)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            caller.read(&memory, |mem_ref| {
                mem_ref.read(payload_ptr, payload.get_mut())?;
                Ok(())
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.send_push(handle, payload.get()),
                |res, mut mem_ref| {
                    let len = res.map(|_| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr, &len.to_le_bytes())
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (reservation_id, destination, value) = caller.read(&memory, |mem_ref| {
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = mem_ref.read_memory_as(rid_pid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;
                Ok((reservation_id.into(), destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reservation_send(
                        reservation_id,
                        HandlePacket::new(destination, payload, value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (reservation_id, destination, value) = caller.read(&memory, |mem_ref| {
                let TwoHashesWithValue {
                    hash1: reservation_id,
                    hash2: destination,
                    value,
                } = mem_ref.read_memory_as(rid_pid_value_ptr)?;
                Ok((reservation_id.into(), destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reservation_send_commit(
                        reservation_id,
                        handle,
                        HandlePacket::new(destination, Default::default(), value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
                         err_ptr: u32|
              -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let last_idx = match at.checked_add(len) {
                Some(i) => i,
                None => {
                    let err = FuncError::ReadLenOverflow(at, len);
                    let size = Encode::encoded_size(&err) as u32;
                    return if let Err(err) =
                        caller.memory(&memory).write(err_ptr, &size.to_le_bytes())
                    {
                        caller.host_state_mut().err = err.into();
                        Err(Trap::from(TrapCode::Unreachable))
                    } else {
                        caller.host_state_mut().err = err;
                        caller.update_globals()?;
                        Ok(())
                    };
                }
            };

            let call_result = caller.host_state_mut().ext.read();
            let message = match call_result {
                Ok(m) => m,
                Err(e) => {
                    caller.host_state_mut().err = FuncError::Core(e);
                    caller.update_globals()?;
                    return Err(TrapCode::Unreachable.into());
                }
            };

            if last_idx > message.len() as u32 {
                let err = FuncError::ReadWrongRange(at..last_idx, message.len() as u32);
                let size = Encode::encoded_size(&err) as u32;
                return if let Err(err) = caller.memory(&memory).write(err_ptr, &size.to_le_bytes())
                {
                    caller.host_state_mut().err = err.into();
                    Err(Trap::from(TrapCode::Unreachable))
                } else {
                    caller.host_state_mut().err = err;
                    caller.update_globals()?;
                    Ok(())
                };
            }

            // non critical copy due to non-production backend
            let message = message[at as usize..last_idx as usize].to_vec();
            match caller.memory(&memory).write(buffer_ptr, &message) {
                Ok(()) => caller.update_globals(),
                Err(e) => {
                    caller.host_state_mut().err = e.into();
                    Err(Trap::from(TrapCode::Unreachable))
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn size(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, length_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.size(),
                |res, mut mem_ref| mem_ref.write(length_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn exit(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, inheritor_id_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let inheritor_id = caller.read(&memory, |mem_ref| {
                mem_ref.read_memory_decoded(inheritor_id_ptr)
            })?;

            caller.host_state_mut().ext.exit().map_err(|e| {
                caller.host_state_mut().err = FuncError::Core(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            // Required here due to post processing query of globals.
            caller.update_globals()?;

            caller.host_state_mut().err =
                FuncError::Terminated(TerminationReason::Exit(inheritor_id));
            Err(TrapCode::Unreachable.into())
        };

        Func::wrap(store, func)
    }

    pub fn status_code(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_code_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.status_code(),
                |res, mut mem_ref| {
                    let err_code = res
                        .map(|code| LengthWithCode {
                            code,
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithCode {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_code_ptr, err_code)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn alloc(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, pages: u32| -> FnResult<u32> {
            let pages =
                WasmPageNumber::new(pages).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

            let caller = CallerWrap::prepare(caller, forbidden)?;
            let mut caller = caller.into_inner();

            let mut host_state = caller.host_data_mut().take();
            let mut mem_ref = MemoryWrapRef {
                memory,
                store: caller.as_context_mut(),
            };

            let page = host_state
                .as_mut()
                .expect("alloc; state should be set")
                .ext
                .alloc(pages, &mut mem_ref);

            *caller.host_data_mut() = host_state;

            let mut caller = CallerWrap::from_inner(caller);
            match page {
                Ok(page) => {
                    log::debug!("ALLOC: {:?} pages at {:?}", pages, page);
                    caller.update_globals()?;
                    Ok((page.raw(),))
                }
                Err(e) => {
                    caller.host_state_mut().err = FuncError::Core(e);
                    Err(Trap::from(TrapCode::Unreachable))
                }
            }
        };

        Func::wrap(store, func)
    }

    pub fn free(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, page: u32| -> EmptyOutput {
            let page = WasmPageNumber::new(page).map_err(|_| Trap::Code(TrapCode::Unreachable))?;

            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            if let Err(e) = caller.host_state_mut().ext.free(page) {
                log::debug!("FREE ERROR: {e}");
                caller.host_state_mut().err = FuncError::Core(e);

                return Err(Trap::from(TrapCode::Unreachable));
            }

            log::debug!("FREE: {page:?}");
            caller.update_globals()?;

            Ok(())
        };

        Func::wrap(store, func)
    }

    pub fn block_height(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, height_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.block_height(),
                |res, mut mem_ref| mem_ref.write(height_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn block_timestamp(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, timestamp_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.block_timestamp(),
                |res, mut mem_ref| mem_ref.write(timestamp_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn origin(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, origin_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.origin(),
                |res, mut mem_ref| mem_ref.write(origin_ptr, res.as_ref()),
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let value = caller.read(&memory, |mem_ref| {
                mem_ref.read(payload_ptr, payload.get_mut())?;

                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply(ReplyPacket::new(payload, value), delay),
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let value = caller.read(&memory, |mem_ref| {
                mem_ref.read(payload_ptr, payload.get_mut())?;

                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply(ReplyPacket::new_with_gas(payload, gas_limit, value), delay),
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let value = caller.read(&memory, |mem_ref| {
                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply_commit(ReplyPacket::new(Default::default(), value), delay),
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let value = caller.read(&memory, |mem_ref| {
                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = Payload::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (reservation_id, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = mem_ref.read_memory_as(rid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;
                Ok((reservation_id.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reservation_reply(reservation_id, ReplyPacket::new(payload, value), delay)
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (reservation_id, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: reservation_id,
                    value,
                } = mem_ref.read_memory_as(rid_value_ptr)?;
                Ok((reservation_id.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reservation_reply_commit(
                        reservation_id,
                        ReplyPacket::new(Default::default(), value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn reply_to(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, err_mid_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply_to(),
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut payload = RuntimeBuffer::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            caller.read(&memory, |mem_ref| {
                mem_ref.read(payload_ptr, payload.get_mut())
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply_push(payload.get()),
                |res, mut mem_ref| {
                    let len = res.map(|_| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr, &len.to_le_bytes())
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let value = caller.read(&memory, |mem_ref| {
                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reply_push_input(offset, len)?;
                    ext.reply_commit(ReplyPacket::new(Default::default(), value), delay)
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.reply_push_input(offset, len),
                |res, mut mem_ref| {
                    let len = res.map(|_| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr as usize, &len.to_le_bytes())
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let value = caller.read(&memory, |mem_ref| {
                if value_ptr as i32 == i32::MAX {
                    Ok(0)
                } else {
                    mem_ref.read_memory_decoded(value_ptr)
                }
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.reply_push_input(offset, len)?;
                    ext.reply_commit(
                        ReplyPacket::new_with_gas(Default::default(), gas_limit, value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    let handle = ext.send_init()?;
                    ext.send_push_input(handle, offset, len)?;
                    ext.send_commit(
                        handle,
                        HandlePacket::new(destination, Default::default(), value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.send_push_input(handle, offset, len),
                |res, mut mem_ref| {
                    let len = res.map(|_| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr as usize, &len.to_le_bytes())
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let (destination, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: destination,
                    value,
                } = mem_ref.read_memory_as(pid_value_ptr)?;

                Ok((destination.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    let handle = ext.send_init()?;
                    ext.send_push_input(handle, offset, len)?;
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
                |res, mut mem_ref| {
                    let err_mid = res
                        .map(|message_id| LengthWithHash {
                            hash: message_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_ptr, err_mid)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn debug(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, string_ptr: u32, len: u32| -> EmptyOutput {
                let mut caller = CallerWrap::prepare(caller, forbidden)?;

                let mut buffer = RuntimeBuffer::try_new_default(len as usize).map_err(|e| {
                    caller.host_state_mut().err = FuncError::RuntimeBufferSize(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

                caller.read(&memory, |mem_ref| {
                    mem_ref.read(string_ptr, buffer.get_mut())
                })?;

                let string = core::str::from_utf8(buffer.get()).map_err(|_| {
                    caller.host_state_mut().err = FuncError::DebugStringParsing;
                    Trap::from(TrapCode::Unreachable)
                })?;

                caller.host_state_mut().ext.debug(string).map_err(|e| {
                    caller.host_state_mut().err = FuncError::Core(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

                caller.update_globals()?;

                Ok(())
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.reserve_gas(gas, duration),
                |res, mut mem_ref| {
                    let err_rid = res
                        .map(|reservation_id| LengthWithHash {
                            hash: reservation_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithHash {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_rid_ptr, err_rid)
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let reservation_id = caller.read(&memory, |mem_ref| {
                mem_ref.read_memory_decoded(reservation_id_ptr)
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.unreserve_gas(reservation_id),
                |res, mut mem_ref| {
                    let err_unreserved = res
                        .map(|gas| LengthWithGas {
                            gas,
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithGas {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_unreserved_ptr, err_unreserved)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_fallible(
                &memory,
                |ext| ext.system_reserve_gas(gas),
                |res, mut mem_ref| {
                    let len = res.map(|()| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr, &len.to_le_bytes())
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn gas_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, gas_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.gas_available(),
                |res, mut mem_ref| mem_ref.write(gas_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn message_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, message_id_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.message_id(),
                |res, mut mem_ref| mem_ref.write(message_id_ptr, res.as_ref()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn program_id(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, program_id_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.program_id(),
                |res, mut mem_ref| mem_ref.write(program_id_ptr, res.as_ref()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn source(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, source_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.source(),
                |res, mut mem_ref| mem_ref.write(source_ptr, res.as_ref()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn value(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.value(),
                |res, mut mem_ref| mem_ref.write(value_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn value_available(
        store: &mut Store<HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, value_ptr: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.call_infallible(
                &memory,
                |ext| ext.value_available(),
                |res, mut mem_ref| mem_ref.write(value_ptr, &res.to_le_bytes()),
            )
        };

        Func::wrap(store, func)
    }

    pub fn random(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         subject_ptr: u32,
                         len: u32,
                         bn_random_ptr: u32|
              -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut subject = RuntimeBuffer::try_new_default(len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            caller.read(&memory, |mem_ref| {
                mem_ref.read(subject_ptr, subject.get_mut())
            })?;

            let (random, bn) = caller.host_state_mut().ext.random();

            subject.try_extend_from_slice(random).map_err(|e| {
                caller.host_state_mut().err = FuncError::RuntimeBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            caller.call_infallible(
                &memory,
                |_ext| Ok(()),
                |_res, mut mem_ref| {
                    let mut hash = [0; 32];
                    hash.copy_from_slice(blake2b(32, &[], subject.get()).as_bytes());

                    mem_ref.write_memory_as(bn_random_ptr, BlockNumberWithHash { bn, hash })
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn leave(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.host_state_mut().ext.leave().map_err(|e| {
                caller.host_state_mut().err = FuncError::Core(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            caller.host_state_mut().err = FuncError::Terminated(TerminationReason::Leave);
            Err(Trap::from(TrapCode::Unreachable))
        };

        Func::wrap(store, func)
    }

    pub fn wait(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller.host_state_mut().ext.wait().map_err(|e| {
                caller.host_state_mut().err = FuncError::Core(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            // Required here due to post processing query of globals.
            caller.update_globals()?;

            caller.host_state_mut().err =
                FuncError::Terminated(TerminationReason::Wait(None, MessageWaitedType::Wait));

            Err(Trap::from(TrapCode::Unreachable))
        };

        Func::wrap(store, func)
    }

    pub fn wait_for(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            caller
                .host_state_mut()
                .ext
                .wait_for(duration)
                .map_err(|e| {
                    caller.host_state_mut().err = FuncError::Core(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

            // Required here due to post processing query of globals.
            caller.update_globals()?;

            caller.host_state_mut().err = FuncError::Terminated(TerminationReason::Wait(
                Some(duration),
                MessageWaitedType::WaitFor,
            ));

            Err(Trap::from(TrapCode::Unreachable))
        };

        Func::wrap(store, func)
    }

    pub fn wait_up_to(store: &mut Store<HostState<E>>, forbidden: bool) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>, duration: u32| -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let enough = caller
                .host_state_mut()
                .ext
                .wait_up_to(duration)
                .map_err(|e| {
                    caller.host_state_mut().err = FuncError::Core(e);
                    Trap::from(TrapCode::Unreachable)
                })?;

            // Required here due to post processing query of globals.
            caller.update_globals()?;

            caller.host_state_mut().err = FuncError::Terminated(TerminationReason::Wait(
                Some(duration),
                if enough {
                    MessageWaitedType::WaitUpToFull
                } else {
                    MessageWaitedType::WaitUpTo
                },
            ));

            Err(Trap::from(TrapCode::Unreachable))
        };

        Func::wrap(store, func)
    }

    pub fn wake(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func = move |caller: Caller<'_, HostState<E>>,
                         message_id_ptr: u32,
                         delay: u32,
                         err_ptr: u32|
              -> EmptyOutput {
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let message_id = caller.read(&memory, |mem_ref| {
                mem_ref.read_memory_decoded(message_id_ptr)
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.wake(message_id, delay),
                |res, mut mem_ref| {
                    let len = res.map(|_| 0).unwrap_or_else(|e| e);
                    mem_ref.write(err_ptr, &len.to_le_bytes())
                },
            )
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut salt = Payload::try_new_default(salt_len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (code_id, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: code_id,
                    value,
                } = mem_ref.read_memory_as(cid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;
                mem_ref.read(salt_ptr, salt.get_mut())?;

                Ok((code_id.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| ext.create_program(InitPacket::new(code_id, salt, payload, value), delay),
                |res, mut mem_ref| {
                    let err_mid_pid = res
                        .map(|(message_id, program_id)| LengthWithTwoHashes {
                            hash1: message_id.into(),
                            hash2: program_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithTwoHashes {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_pid_ptr, err_mid_pid)
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
            let mut caller = CallerWrap::prepare(caller, forbidden)?;

            let mut salt = Payload::try_new_default(salt_len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let mut payload = Payload::try_new_default(payload_len as usize).map_err(|e| {
                caller.host_state_mut().err = FuncError::PayloadBufferSize(e);
                Trap::from(TrapCode::Unreachable)
            })?;

            let (code_id, value) = caller.read(&memory, |mem_ref| {
                let HashWithValue {
                    hash: code_id,
                    value,
                } = mem_ref.read_memory_as(cid_value_ptr)?;
                mem_ref.read(payload_ptr, payload.get_mut())?;
                mem_ref.read(salt_ptr, salt.get_mut())?;

                Ok((code_id.into(), value))
            })?;

            caller.call_fallible(
                &memory,
                |ext| {
                    ext.create_program(
                        InitPacket::new_with_gas(code_id, salt, payload, gas_limit, value),
                        delay,
                    )
                },
                |res, mut mem_ref| {
                    let err_mid_pid = res
                        .map(|(message_id, program_id)| LengthWithTwoHashes {
                            hash1: message_id.into(),
                            hash2: program_id.into(),
                            ..Default::default()
                        })
                        .unwrap_or_else(|length| LengthWithTwoHashes {
                            length,
                            ..Default::default()
                        });

                    mem_ref.write_memory_as(err_mid_pid_ptr, err_mid_pid)
                },
            )
        };

        Func::wrap(store, func)
    }

    pub fn error(store: &mut Store<HostState<E>>, forbidden: bool, memory: WasmiMemory) -> Func {
        let func =
            move |caller: Caller<'_, HostState<E>>, error_ptr: u32, err_ptr: u32| -> EmptyOutput {
                let mut caller = CallerWrap::prepare(caller, forbidden)?;

                caller.call_fallible(
                    &memory,
                    |ext| ext.last_error().map(Encode::encode),
                    |res, mut mem_ref| {
                        let len = match res {
                            Ok(err) => {
                                mem_ref.write(error_ptr, err.as_ref())?;
                                0
                            }
                            Err(e) => e,
                        };

                        mem_ref.write(err_ptr, &len.to_le_bytes())
                    },
                )
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
