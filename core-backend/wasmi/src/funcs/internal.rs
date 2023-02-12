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

use codec::MaxEncodedLen;
use gear_backend_common::{
    memory::{
        MemoryAccessManager, MemoryOwner, WasmMemoryRead, WasmMemoryReadAs, WasmMemoryReadDecoded,
        WasmMemoryWrite, WasmMemoryWriteAs,
    },
    BackendExt, BackendState,
};

use super::*;
use crate::state::State;

pub fn caller_host_state_mut<'a, 'b: 'a, E>(caller: &'a mut Caller<'b, Option<E>>) -> &'a mut E {
    caller
        .host_data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub fn caller_host_state_take<'a, 'b: 'a, E>(caller: &'a mut Caller<'b, Option<E>>) -> E {
    caller
        .host_data_mut()
        .take()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub(crate) struct CallerWrap<'a, E>
where
    E: Ext + 'static,
    E::Error: BackendExtError,
{
    caller: Caller<'a, HostState<E>>,
    manager: MemoryAccessManager<E>,
    memory: WasmiMemory,
}

impl<'a, E> CallerWrap<'a, E>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
{
    /// !!! Usage warning: make sure to do it before any other read/write,
    /// because it may contain register read.
    pub(crate) fn register_and_read_value(
        &mut self,
        value_ptr: u32,
    ) -> Result<u128, MemoryAccessError> {
        if value_ptr != PTR_SPECIAL {
            let read_value = self.register_read_decoded(value_ptr);
            return self.read_decoded(read_value);
        }

        Ok(0)
    }

    #[track_caller]
    pub fn prepare(
        caller: Caller<'a, HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Result<Self, Trap> {
        let mut wrapper = Self {
            caller,
            manager: Default::default(),
            memory,
        };

        if forbidden {
            wrapper
                .host_state_mut()
                .set_termination_reason(E::Error::forbidden_function().into_termination_reason());

            return Err(TrapCode::Unreachable.into());
        }

        let f = || {
            let gas_global = wrapper.caller.get_export(GLOBAL_NAME_GAS)?.into_global()?;
            let gas = gas_global.get(&wrapper.caller).try_into::<i64>()? as u64;

            let allowance_global = wrapper
                .caller
                .get_export(GLOBAL_NAME_ALLOWANCE)?
                .into_global()?;
            let allowance = allowance_global.get(&wrapper.caller).try_into::<i64>()? as u64;

            Some((gas, allowance))
        };

        let (gas, allowance) =
            f().unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        wrapper.host_state_mut().ext.update_counters(gas, allowance);

        Ok(wrapper)
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<E> {
        caller_host_state_mut(&mut self.caller)
    }

    #[track_caller]
    pub fn memory(&mut self) -> MemoryWrapRef<'_, E> {
        let store = self.caller.as_context_mut();
        MemoryWrapRef {
            memory: self.memory,
            store,
        }
    }

    fn with_state_taken<F, T, Err>(&mut self, f: F) -> Result<T, Err>
    where
        F: FnOnce(&mut Self, &mut State<E>) -> Result<T, Err>,
    {
        let mut state = caller_host_state_take(&mut self.caller);

        let res = f(self, &mut state);

        self.caller.host_data_mut().replace(state);

        res
    }

    fn update_globals(&mut self) {
        let (gas, allowance) = self.host_state_mut().ext.counters();

        let mut f = || {
            let gas_global = self.caller.get_export(GLOBAL_NAME_GAS)?.into_global()?;
            gas_global
                .set(&mut self.caller, Value::I64(gas as i64))
                .ok()?;

            let allowance_global = self
                .caller
                .get_export(GLOBAL_NAME_ALLOWANCE)?
                .into_global()?;
            allowance_global
                .set(&mut self.caller, Value::I64(allowance as i64))
                .ok()?;

            Some(())
        };

        f().unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));
    }

    #[track_caller]
    pub fn run<T, F>(&mut self, f: F) -> Result<T, Trap>
    where
        F: FnOnce(&mut Self) -> Result<T, FuncError<E::Error>>,
    {
        let result = f(self).map_err(|err| {
            self.host_state_mut().set_termination_reason(err.into());
            Trap::from(TrapCode::Unreachable)
        });

        self.update_globals();

        result
    }

    #[track_caller]
    pub fn run_fallible<T: Sized, F, R>(&mut self, res_ptr: u32, f: F) -> Result<(), Trap>
    where
        F: FnOnce(&mut Self) -> Result<T, FuncError<E::Error>>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run(|ctx: &mut Self| -> Result<_, FuncError<E::Error>> {
            let res = f(ctx);
            let res = ctx.host_state_mut().process_fallible_func_result(res)?;

            // TODO: move above or make normal process memory access.
            let write_res = ctx.register_write_as::<R>(res_ptr);

            ctx.write_as(write_res, R::from(res)).map_err(Into::into)
        })
    }

    // pub fn run<T, F>(&mut self, f: F) -> Result<T, Trap>
    // where
    //     F: FnOnce(&mut Self) -> Result<T, FuncError<E::Error>>,
    // {
    //     self.run_internal(f)
    // }

    #[track_caller]
    pub fn run_fallible_state_taken<T: Sized, F, R>(
        &mut self,
        res_ptr: u32,
        f: F,
    ) -> Result<(), Trap>
    where
        F: FnOnce(&mut Self, &mut State<E>) -> Result<T, FuncError<E::Error>>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run(|ctx| {
            let res = ctx.with_state_taken(f);
            let res = ctx.host_state_mut().process_fallible_func_result(res)?;

            // TODO: move above or make normal process memory access.
            let write_res = ctx.register_write_as::<R>(res_ptr);

            ctx.write_as(write_res, R::from(res)).map_err(Into::into)
        })
    }

    #[track_caller]
    pub fn run_state_taken<T, F>(&mut self, f: F) -> Result<T, Trap>
    where
        F: FnOnce(&mut Self, &mut State<E>) -> Result<T, FuncError<E::Error>>,
    {
        self.run(|ctx| ctx.with_state_taken(f))
    }
}

impl<'a, E> MemoryAccessRecorder for CallerWrap<'a, E>
where
    E: Ext + 'static,
    E::Error: BackendExtError,
{
    fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        self.manager.register_read(ptr, size)
    }

    fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        self.manager.register_read_as(ptr)
    }

    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T> {
        self.manager.register_read_decoded(ptr)
    }

    fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        self.manager.register_write(ptr, size)
    }

    fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        self.manager.register_write_as(ptr)
    }
}

impl<'a, E> MemoryOwner for CallerWrap<'a, E>
where
    E: BackendExt + 'static,
    E::Error: BackendExtError,
{
    fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.read(&memory, read)
    }

    fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.read_as(&memory, read)
    }

    fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.read_decoded(&memory, read)
    }

    fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let mut memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.write(&mut memory, write, buff)
    }

    fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let mut memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.write_as(&mut memory, write, obj)
    }
}
