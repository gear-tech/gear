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
use crate::state::State;

pub(crate) struct CallerWrap<'a, E>(Caller<'a, HostState<E>>)
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError;

impl<'a, E> CallerWrap<'a, E>
where
    E: Ext + IntoExtInfo<E::Error> + 'static,
    E::Error: IntoExtError,
{
    #[track_caller]
    pub fn prepare(caller: Caller<'a, HostState<E>>, forbidden: bool) -> Result<Self, Trap> {
        let mut caller = Self(caller);

        if forbidden {
            caller.host_state_mut().err = FuncError::Core(E::Error::forbidden_function());
            return Err(TrapCode::Unreachable.into());
        }

        let f = || {
            let gas_global = caller.0.get_export(GLOBAL_NAME_GAS)?.into_global()?;
            let gas = gas_global.get(&caller.0).try_into::<i64>()? as u64;

            let allowance_global = caller.0.get_export(GLOBAL_NAME_ALLOWANCE)?.into_global()?;
            let allowance = allowance_global.get(&caller.0).try_into::<i64>()? as u64;

            Some((gas, allowance))
        };

        let (gas, allowance) = f().ok_or_else(|| {
            caller.host_state_mut().err = FuncError::HostError;
            Trap::from(TrapCode::Unreachable)
        })?;

        caller.host_state_mut().ext.update_counters(gas, allowance);

        Ok(caller)
    }

    pub fn into_inner(self) -> Caller<'a, HostState<E>> {
        self.0
    }

    pub fn from_inner(caller: Caller<'a, HostState<E>>) -> Self {
        Self(caller)
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<E> {
        self.0
            .host_data_mut()
            .as_mut()
            .expect("host_state should be set before execution")
    }

    #[track_caller]
    pub fn memory(&mut self, memory: &WasmiMemory) -> MemoryWrapRef<'_, E> {
        let store = self.0.as_context_mut();
        MemoryWrapRef {
            memory: *memory,
            store,
        }
    }

    #[track_caller]
    pub fn update_globals(&mut self) -> Result<(), Trap> {
        let (gas, allowance) = self.host_state_mut().ext.counters();

        let mut f = || {
            let gas_global = self.0.get_export(GLOBAL_NAME_GAS)?.into_global()?;
            gas_global.set(&mut self.0, Value::I64(gas as i64)).ok()?;

            let allowance_global = self.0.get_export(GLOBAL_NAME_ALLOWANCE)?.into_global()?;
            allowance_global
                .set(&mut self.0, Value::I64(allowance as i64))
                .ok()?;

            Some(())
        };

        f().ok_or_else(|| {
            self.host_state_mut().err = FuncError::HostError;
            Trap::from(TrapCode::Unreachable)
        })
    }

    #[track_caller]
    pub fn read<T, F>(&mut self, memory: &WasmiMemory, f: F) -> Result<T, Trap>
    where
        F: FnOnce(MemoryWrapRef<'_, E>) -> Result<T, MemoryError>,
    {
        let memory_ref = self.memory(memory);

        f(memory_ref).map_err(|e| {
            self.host_state_mut().err = e.into();
            Trap::from(TrapCode::Unreachable)
        })
    }

    #[track_caller]
    pub fn call_fallible<T, Call, Res>(
        &mut self,
        memory: &WasmiMemory,
        call: Call,
        result: Res,
    ) -> Result<(), Trap>
    where
        Call: FnOnce(&mut E) -> Result<T, <E as Ext>::Error>,
        Res: FnOnce(Result<T, u32>, MemoryWrapRef<'_, E>) -> Result<(), MemoryError>,
    {
        let res = match call(&mut self.host_state_mut().ext) {
            Ok(value) => Ok(value),
            Err(e) => match e.into_ext_error() {
                Ok(ext_error) => Err(ext_error.encoded_size() as u32),
                Err(e) => {
                    self.host_state_mut().err = FuncError::Core(e);
                    return Err(Trap::from(TrapCode::Unreachable));
                }
            },
        };

        result(res, self.memory(memory)).map_err(|e| {
            self.host_state_mut().err = e.into();
            Trap::from(TrapCode::Unreachable)
        })?;

        self.update_globals()?;

        Ok(())
    }

    #[track_caller]
    pub fn call_infallible<T, Call, Res>(
        &mut self,
        memory: &WasmiMemory,
        call: Call,
        result: Res,
    ) -> Result<(), Trap>
    where
        Call: FnOnce(&mut E) -> Result<T, <E as Ext>::Error>,
        Res: FnOnce(T, MemoryWrapRef<'_, E>) -> Result<(), MemoryError>,
    {
        let res = call(&mut self.host_state_mut().ext).map_err(|e| {
            self.host_state_mut().err = FuncError::Core(e);
            Trap::from(TrapCode::Unreachable)
        })?;

        result(res, self.memory(memory)).map_err(|e| {
            self.host_state_mut().err = e.into();
            Trap::from(TrapCode::Unreachable)
        })?;

        self.update_globals()?;

        Ok(())
    }
}
