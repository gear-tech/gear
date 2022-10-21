// This file is part of Gear.

// Copyright (C) 2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-lat&er WITH Classpath-exception-2.0

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

use super::*;

pub(super) enum Error {
    HostStateNone,
    Trap(Trap),
}

macro_rules! host_state_mut {
    ($caller:ident) => {
        $caller
            .host_data_mut()
            .as_mut()
            .expect("host_state should be set before execution")
    };
}

macro_rules! update_globals {
    ($caller:ident) => {{
        let (gas, allowance) = host_state_mut!($caller).ext.counters();

        match $caller
            .get_export(GLOBAL_NAME_GAS)
            .and_then(Extern::into_global)
            .and_then(|g| g.set(&mut $caller, Value::I64(gas as i64)).ok())
            .and_then(|_| $caller.get_export(GLOBAL_NAME_ALLOWANCE))
            .and_then(Extern::into_global)
            .and_then(|g| g.set(&mut $caller, Value::I64(allowance as i64)).ok())
        {
            Some(_) => Ok(()),
            None => {
                host_state_mut!($caller).err = FuncError::HostError;
                Err(Trap::from(TrapCode::Unreachable))
            }
        }
    }};
}

pub(super) use host_state_mut;
pub(super) use update_globals;

pub(super) fn process_call_unit_result<E, CallType>(
    mut caller: Caller<'_, HostState<E>>,
    call: CallType,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
    CallType: Fn(&mut E) -> Result<(), <E as Ext>::Error>,
{
    let host_state = caller
        .host_data_mut()
        .as_mut()
        .ok_or(Error::HostStateNone)?;

    let call_result = call(&mut host_state.ext);
    let result = match call_result {
        Ok(_) => Ok((0u32,)),
        Err(e) => match e.into_ext_error() {
            Ok(ext_error) => Ok((ext_error.encoded_size() as u32,)),
            Err(e) => {
                host_state.err = FuncError::Core(e);
                Err(Error::Trap(TrapCode::Unreachable.into()))
            }
        },
    };

    update_globals!(caller).map_err(Error::Trap).and(result)
}

pub(super) fn process_call_result<E, ResultType, CallType, WriteType>(
    mut caller: Caller<'_, HostState<E>>,
    memory: WasmiMemory,
    call: CallType,
    write: WriteType,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
    CallType: FnOnce(&mut E) -> Result<ResultType, <E as Ext>::Error>,
    WriteType: Fn(&mut MemoryWrapRef<'_, E>, ResultType) -> Result<(), MemoryError>,
{
    let host_state = caller
        .host_data_mut()
        .as_mut()
        .ok_or(Error::HostStateNone)?;

    let call_result = call(&mut host_state.ext);
    let result = match call_result {
        Ok(return_value) => {
            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                write(&mut memory_wrap, return_value)
            };

            match write_result {
                Ok(_) => Ok((0u32,)),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(Error::Trap(TrapCode::Unreachable.into()))
                }
            }
        }
        Err(e) => match e.into_ext_error() {
            Ok(ext_error) => Ok((ext_error.encoded_size() as u32,)),
            Err(e) => {
                host_state.err = FuncError::Core(e);
                Err(Error::Trap(TrapCode::Unreachable.into()))
            }
        },
    };

    update_globals!(caller).map_err(Error::Trap).and(result)
}

pub(super) fn process_call_result_as_ref<E, ResultType, CallType>(
    caller: Caller<'_, HostState<E>>,
    memory: WasmiMemory,
    call: CallType,
    offset: u32,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
    ResultType: AsRef<[u8]>,
    CallType: FnOnce(&mut E) -> Result<ResultType, <E as Ext>::Error>,
{
    process_call_result(caller, memory, call, |memory_wrap, result| {
        memory_wrap.write(offset as usize, result.as_ref())
    })
}

pub(super) fn process_infalliable_call<E, ResultType, CallType, WriteType>(
    mut caller: Caller<'_, HostState<E>>,
    memory: WasmiMemory,
    call: CallType,
    write: WriteType,
) -> Result<(), Error>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
    CallType: FnOnce(&mut E) -> Result<ResultType, <E as Ext>::Error>,
    WriteType: Fn(&mut MemoryWrapRef<'_, E>, ResultType) -> Result<(), MemoryError>,
{
    let host_state = caller
        .host_data_mut()
        .as_mut()
        .ok_or(Error::HostStateNone)?;

    let call_result = call(&mut host_state.ext);
    let result = match call_result {
        Ok(return_value) => {
            let write_result = {
                let mut memory_wrap = get_caller_memory(&mut caller, &memory);
                write(&mut memory_wrap, return_value)
            };

            match write_result {
                Ok(_) => Ok(()),
                Err(e) => {
                    host_state_mut!(caller).err = e.into();

                    Err(Error::Trap(TrapCode::Unreachable.into()))
                }
            }
        }
        Err(e) => {
            host_state.err = FuncError::Core(e);
            Err(Error::Trap(TrapCode::Unreachable.into()))
        }
    };

    update_globals!(caller).map_err(Error::Trap).and(result)
}
