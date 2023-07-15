// This file is part of Gear.

// Copyright (C) 2022-2023 Gear Technologies Inc.
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

//! Runtime implementation for wasmi backend.

use crate::{memory::MemoryWrapRef, state::HostState};
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
use gear_backend_common::{
    memory::{
        MemoryAccessError, MemoryAccessManager, MemoryAccessRecorder, MemoryOwner, WasmMemoryRead,
        WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    runtime::{RunFallibleError, Runtime},
    ActorTerminationReason, BackendExternalities, BackendState, TerminationReason, TrapExplanation,
};
use gear_core::{costs::RuntimeCosts, gas::GasLeft, pages::WasmPage};
use gear_wasm_instrument::GLOBAL_NAME_GASCNT;
use wasmi::{
    core::{Trap, TrapCode, Value},
    AsContextMut, Caller, Memory as WasmiMemory,
};

use crate::state::State;

pub(crate) fn caller_host_state_mut<'a, 'b: 'a, Ext>(
    caller: &'a mut Caller<'b, Option<Ext>>,
) -> &'a mut Ext {
    caller
        .host_data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub(crate) fn caller_host_state_take<'a, 'b: 'a, Ext>(
    caller: &'a mut Caller<'b, Option<Ext>>,
) -> Ext {
    caller
        .host_data_mut()
        .take()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub(crate) struct CallerWrap<'a, Ext> {
    pub caller: Caller<'a, HostState<Ext>>,
    pub manager: MemoryAccessManager<Ext>,
    pub memory: WasmiMemory,
}

impl<'a, Ext: BackendExternalities + 'static> Runtime<Ext> for CallerWrap<'a, Ext> {
    type Error = Trap;

    fn ext_mut(&mut self) -> &mut Ext {
        &mut self
            .caller
            .host_data_mut()
            .as_mut()
            .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
            .ext
    }

    fn unreachable_error() -> Self::Error {
        Trap::Code(TrapCode::Unreachable)
    }

    #[track_caller]
    fn run_any<T, F>(&mut self, cost: RuntimeCosts, f: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, TerminationReason>,
    {
        self.with_globals_update(|ctx| {
            ctx.host_state_mut().ext.charge_gas_runtime(cost)?;
            f(ctx)
        })
    }

    #[track_caller]
    fn run_fallible<T: Sized, F, R>(
        &mut self,
        res_ptr: u32,
        cost: RuntimeCosts,
        f: F,
    ) -> Result<(), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, RunFallibleError>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run_any(cost, |ctx: &mut Self| -> Result<_, TerminationReason> {
            let res = f(ctx);
            let res = ctx.host_state_mut().process_fallible_func_result(res)?;

            // TODO: move above or make normal process memory access.
            let write_res = ctx.register_write_as::<R>(res_ptr);

            ctx.write_as(write_res, R::from(res)).map_err(Into::into)
        })
    }

    fn alloc(&mut self, pages: u32) -> Result<WasmPage, <Ext>::AllocError> {
        let mut state = caller_host_state_take(&mut self.caller);
        let mut mem = CallerWrap::memory(&mut self.caller, self.memory);
        let res = state.ext.alloc(pages, &mut mem);
        self.caller.host_data_mut().replace(state);
        res
    }
}

impl<'a, Ext: BackendExternalities + 'static> CallerWrap<'a, Ext> {
    #[track_caller]
    pub fn prepare(
        caller: Caller<'a, HostState<Ext>>,
        forbidden: bool,
        memory: WasmiMemory,
    ) -> Result<Self, Trap> {
        let mut wrapper = Self {
            caller,
            manager: Default::default(),
            memory,
        };

        if forbidden {
            wrapper.host_state_mut().set_termination_reason(
                ActorTerminationReason::Trap(TrapExplanation::ForbiddenFunction).into(),
            );

            return Err(TrapCode::Unreachable.into());
        }

        let f = || {
            let gascnt_global = wrapper
                .caller
                .get_export(GLOBAL_NAME_GASCNT)?
                .into_global()?;

            Some(gascnt_global.get(&wrapper.caller).try_into::<i64>()? as u64)
        };

        let gas_left =
            f().unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        wrapper.host_state_mut().ext.decrease_to(gas_left);

        Ok(wrapper)
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<Ext> {
        caller_host_state_mut(&mut self.caller)
    }

    #[track_caller]
    pub fn memory<'b, 'c: 'b>(
        caller: &'b mut Caller<'c, Option<State<Ext>>>,
        memory: WasmiMemory,
    ) -> MemoryWrapRef<'b, Ext> {
        MemoryWrapRef::<'b, _> {
            memory,
            store: caller.as_context_mut(),
        }
    }

    fn update_globals(&mut self) {
        let gascnt = self.host_state_mut().ext.define_actual();

        let mut f = || {
            let gascnt_global = self.caller.get_export(GLOBAL_NAME_GASCNT)?.into_global()?;
            gascnt_global
                .set(&mut self.caller, Value::I64(gascnt as i64))
                .ok()?;
            Some(())
        };

        f().unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));
    }

    fn with_memory<R, F>(&mut self, f: F) -> Result<R, MemoryAccessError>
    where
        F: FnOnce(
            &mut MemoryAccessManager<Ext>,
            &mut MemoryWrapRef<Ext>,
            &mut GasLeft,
        ) -> Result<R, MemoryAccessError>,
    {
        let mut gas_left = self.host_state_mut().ext.gas_left();
        let _ = self.host_state_mut().ext.define_actual();

        let mut memory = Self::memory(&mut self.caller, self.memory);

        // With memory ops do similar subtractions for both counters.
        let res = f(&mut self.manager, &mut memory, &mut gas_left);

        let min = self.host_state_mut().ext.define_actual();
        self.host_state_mut().ext.decrease_to(min);
        res
    }

    fn with_globals_update<T, F>(&mut self, f: F) -> Result<T, Trap>
    where
        F: FnOnce(&mut Self) -> Result<T, TerminationReason>,
    {
        let result = f(self).map_err(|err| {
            self.host_state_mut().set_termination_reason(err);
            Trap::from(TrapCode::Unreachable)
        });

        self.update_globals();

        result
    }
}

impl<'a, Ext> MemoryAccessRecorder for CallerWrap<'a, Ext> {
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

impl<Ext> BackendState for CallerWrap<'_, Ext> {
    fn set_termination_reason(&mut self, reason: TerminationReason) {
        caller_host_state_mut(&mut self.caller).set_termination_reason(reason);
    }
}

impl<'a, Ext: BackendExternalities + 'static> MemoryOwner for CallerWrap<'a, Ext> {
    fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        self.with_memory(|manager, memory, gas_left| manager.read(memory, read, gas_left))
    }

    fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        self.with_memory(|manager, memory, gas_left| manager.read_as(memory, read, gas_left))
    }

    fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.read_decoded(memory, read, gas_left)
        })
    }

    fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.write(memory, write, buff, gas_left)
        })
    }

    fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.write_as(memory, write, obj, gas_left)
        })
    }
}
