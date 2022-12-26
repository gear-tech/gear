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

use core::ops::{Deref, DerefMut};

use codec::MaxEncodedLen;
use gear_backend_common::memory::{
    MemoryAccessManager, WasmMemoryRead, WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite,
    WasmMemoryWriteAs,
};

use super::*;
use crate::state::State;

pub(crate) struct CallerWrap<'a, E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
{
    caller: Caller<'a, HostState<E>>,
    manager: MemoryAccessManager<E>,
    memory: WasmiMemory,
}

impl<'a, E> Deref for CallerWrap<'a, E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
{
    type Target = MemoryAccessManager<E>;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl<'a, E> DerefMut for CallerWrap<'a, E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.manager
    }
}

impl<'a, E> CallerWrap<'a, E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError + Display,
{
    #[track_caller]
    pub fn prepare(
        caller: Caller<'a, HostState<E>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Result<Self, Trap> {
        let mut caller = Self {
            caller,
            manager: Default::default(),
            memory,
        };

        if forbidden {
            caller.host_state_mut().err = FuncError::Core(E::Error::forbidden_function());
            return Err(TrapCode::Unreachable.into());
        }

        let f = || {
            let gas_global = caller.caller.get_export(GLOBAL_NAME_GAS)?.into_global()?;
            let gas = gas_global.get(&caller.caller).try_into::<i64>()? as u64;

            let allowance_global = caller
                .caller
                .get_export(GLOBAL_NAME_ALLOWANCE)?
                .into_global()?;
            let allowance = allowance_global.get(&caller.caller).try_into::<i64>()? as u64;

            Some((gas, allowance))
        };

        let (gas, allowance) = f().ok_or_else(|| {
            caller.host_state_mut().err = FuncError::HostError;
            Trap::from(TrapCode::Unreachable)
        })?;

        caller.host_state_mut().ext.update_counters(gas, allowance);

        Ok(caller)
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<E> {
        self.caller
            .host_data_mut()
            .as_mut()
            .expect("host_state should be set before execution")
    }

    #[track_caller]
    pub fn memory(&mut self) -> MemoryWrapRef<'_, E> {
        let store = self.caller.as_context_mut();
        MemoryWrapRef {
            memory: self.memory,
            store,
        }
    }

    #[track_caller]
    pub fn update_globals(&mut self) -> Result<(), Trap> {
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

        f().ok_or_else(|| {
            self.host_state_mut().err = FuncError::HostError;
            Trap::from(TrapCode::Unreachable)
        })
    }

    #[track_caller]
    pub(crate) fn run<T, F>(&mut self, f: F) -> Result<T, Trap>
    where
        F: FnOnce(&mut Self) -> Result<T, FuncError<E::Error>>,
    {
        let result = f(self).map_err(|err| {
            self.host_state_mut().err = err;
            Trap::from(TrapCode::Unreachable)
        });

        self.update_globals()?;

        result
    }

    #[track_caller]
    pub(crate) fn run_state_taken<T, F>(&mut self, f: F) -> Result<T, Trap>
    where
        F: FnOnce(&mut Self, &mut State<E>) -> Result<T, FuncError<E::Error>>,
    {
        let mut state = self
            .caller
            .host_data_mut()
            .take()
            .expect("State must be set before execution");

        let result = f(self, &mut state);

        self.caller.host_data_mut().replace(state);

        self.update_globals()?;

        result.map_err(|err| {
            self.host_state_mut().err = err;
            Trap::from(TrapCode::Unreachable)
        })
    }

    pub fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.read(&memory, read)
    }

    pub fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.read_as(&memory, read)
    }

    pub fn read_decoded<T: Decode + MaxEncodedLen>(
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

    pub fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError> {
        let store = self.caller.as_context_mut();
        let mut memory = MemoryWrapRef {
            memory: self.memory,
            store,
        };
        self.manager.write(&mut memory, write, buff)
    }

    pub fn write_as<T: Sized>(
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
