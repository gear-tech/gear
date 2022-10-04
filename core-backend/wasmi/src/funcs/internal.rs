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

pub(super) fn process_call_unit_result<E, CallType>(
    mut caller: Caller<'_, HostState<E>>,
    call: CallType,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: IntoExtError,
    CallType: Fn(&mut E) -> Result<(), <E as Ext>::Error>,
{
    let host_state = caller
        .host_data_mut()
        .as_mut()
        .ok_or(Error::HostStateNone)?;

    let call_result = call(&mut host_state.ext);
    match call_result {
        Ok(_) => Ok((0u32,)),
        Err(e) => match e.into_ext_error() {
            Ok(ext_error) => Ok((ext_error.encoded_size() as u32,)),
            Err(e) => {
                host_state.err = FuncError::Core(e);
                Err(Error::Trap(DummyHostError.into()))
            }
        },
    }
}

pub(super) fn process_call_result<E, ResultType, CallType, WriteType>(
    mut caller: Caller<'_, HostState<E>>,
    memory: WasmiMemory,
    call: CallType,
    write: WriteType,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: IntoExtError,
    CallType: FnOnce(&mut E) -> Result<ResultType, <E as Ext>::Error>,
    WriteType: Fn(&mut MemoryWrapRef<'_, E>, ResultType) -> Result<(), MemoryError>,
{
    let host_state = caller
        .host_data_mut()
        .as_mut()
        .ok_or(Error::HostStateNone)?;

    let call_result = call(&mut host_state.ext);
    let return_value = match call_result {
        Ok(r) => r,
        Err(e) => match e.into_ext_error() {
            Ok(ext_error) => {
                return Ok((ext_error.encoded_size() as u32,));
            }
            Err(e) => {
                host_state.err = FuncError::Core(e);
                return Err(Error::Trap(DummyHostError.into()));
            }
        },
    };

    let write_result = {
        let mut memory_wrap = get_caller_memory(&mut caller, &memory);
        write(&mut memory_wrap, return_value)
    };

    match write_result {
        Ok(_) => Ok((0u32,)),
        Err(e) => {
            // this is safe to unwrap since we own the caller, don't change its host_data
            // and checked for abscense before
            caller.host_data_mut().as_mut().unwrap().err = e.into();

            Err(Error::Trap(DummyHostError.into()))
        }
    }
}

pub(super) fn process_call_result_as_ref<E, ResultType, CallType>(
    caller: Caller<'_, HostState<E>>,
    memory: WasmiMemory,
    call: CallType,
    offset: usize,
) -> Result<(u32,), Error>
where
    E: Ext + IntoExtInfo + 'static,
    E::Error: IntoExtError,
    ResultType: AsRef<[u8]>,
    CallType: FnOnce(&mut E) -> Result<ResultType, <E as Ext>::Error>,
{
    process_call_result(caller, memory, call, |memory_wrap, result| {
        memory_wrap.write(offset, result.as_ref())
    })
}
