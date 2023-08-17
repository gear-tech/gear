// This file is part of Gear.

// Copyright (C) 2021-2023 Gear Technologies Inc.
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

//! sp-sandbox runtime (here it's contract execution state) realization.

use crate::{
    memory::MemoryWrapRef,
    state::{HostState, State},
    DefaultExecutorMemory,
};
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
use gear_backend_common::{
    memory::{
        MemoryAccessError, MemoryAccessManager, MemoryAccessRecorder, MemoryOwner, WasmMemoryRead,
        WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    runtime::{RunFallibleError, Runtime as CommonRuntime},
    BackendExternalities, BackendState, UndefinedTerminationReason,
};
use gear_core::{costs::RuntimeCosts, pages::WasmPage};
use gear_sandbox::{default_executor::Caller, AsContext, HostError, SandboxCaller, Value};
use gear_wasm_instrument::GLOBAL_NAME_GAS;

pub(crate) fn as_i64(v: Value) -> Option<i64> {
    match v {
        Value::I64(i) => Some(i),
        _ => None,
    }
}

pub(crate) fn caller_host_state_take<Ext>(caller: &mut Caller<'_, HostState<Ext>>) -> State<Ext> {
    caller
        .data_mut()
        .take()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub(crate) struct CallerWrap<'a, Ext> {
    pub caller: Caller<'a, HostState<Ext>>,
    pub manager: MemoryAccessManager<Ext>,
    pub memory: DefaultExecutorMemory,
}

impl<'a, Ext: BackendExternalities + 'static> CommonRuntime<Ext> for CallerWrap<'a, Ext> {
    type Error = HostError;

    fn ext_mut(&mut self) -> &mut Ext {
        &mut self.host_state_mut().ext
    }

    fn unreachable_error() -> Self::Error {
        HostError
    }

    #[track_caller]
    fn run_any<T, F>(&mut self, gas: u64, cost: RuntimeCosts, f: F) -> Result<(u64, T), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, UndefinedTerminationReason>,
    {
        self.host_state_mut().ext.decrease_current_counter_to(gas);

        let run = || {
            self.host_state_mut().ext.charge_gas_runtime(cost)?;
            f(self)
        };

        run()
            .map_err(|err| {
                self.set_termination_reason(err);
                HostError
            })
            .map(|r| (self.host_state_mut().ext.define_current_counter(), r))
    }

    #[track_caller]
    fn run_fallible<T: Sized, F, R>(
        &mut self,
        gas: u64,
        res_ptr: u32,
        cost: RuntimeCosts,
        f: F,
    ) -> Result<(u64, ()), Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, RunFallibleError>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run_any(
            gas,
            cost,
            |ctx: &mut Self| -> Result<_, UndefinedTerminationReason> {
                let res = f(ctx);
                let res = ctx.host_state_mut().process_fallible_func_result(res)?;

                // TODO: move above or make normal process memory access.
                let write_res = ctx.register_write_as::<R>(res_ptr);

                ctx.write_as(write_res, R::from(res)).map_err(Into::into)
            },
        )
    }

    fn alloc(&mut self, pages: u32) -> Result<WasmPage, <Ext>::AllocError> {
        let mut state = caller_host_state_take(&mut self.caller);
        let mut mem = CallerWrap::memory(&mut self.caller, self.memory.clone());
        let res = state.ext.alloc(pages, &mut mem);
        self.caller.data_mut().replace(state);
        res
    }
}

impl<'a, Ext: BackendExternalities + 'static> CallerWrap<'a, Ext> {
    #[track_caller]
    pub fn prepare(
        caller: Caller<'a, HostState<Ext>>,
        memory: DefaultExecutorMemory,
    ) -> Result<Self, HostError> {
        let mut wrapper = Self {
            caller,
            manager: Default::default(),
            memory,
        };

        let f = || {
            let gas = wrapper.caller.get_global_val(GLOBAL_NAME_GAS)?;
            let Value::I64(gas) = gas else { return None };
            Some(gas as u64)
        };

        let gas_left =
            f().unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        wrapper
            .host_state_mut()
            .ext
            .decrease_current_counter_to(gas_left);

        Ok(wrapper)
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<Ext> {
        self.caller
            .data_mut()
            .as_mut()
            .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
    }

    #[track_caller]
    pub fn memory<'b, 'c: 'b>(
        caller: &'b mut Caller<'c, HostState<Ext>>,
        memory: DefaultExecutorMemory,
    ) -> MemoryWrapRef<'b, 'c, Ext> {
        MemoryWrapRef::<'b, 'c, _> { memory, caller }
    }

    fn with_memory<R, F>(&mut self, f: F) -> Result<R, MemoryAccessError>
    where
        F: FnOnce(
            &mut MemoryAccessManager<Ext>,
            &mut MemoryWrapRef<Ext>,
            &mut u64,
        ) -> Result<R, MemoryAccessError>,
    {
        let mut gas_counter = self.host_state_mut().ext.define_current_counter();

        let mut memory = Self::memory(&mut self.caller, self.memory.clone());

        // With memory ops do similar subtractions for both counters.
        let res = f(&mut self.manager, &mut memory, &mut gas_counter);

        self.host_state_mut()
            .ext
            .decrease_current_counter_to(gas_counter);
        res
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

impl<Ext: BackendExternalities + 'static> BackendState for CallerWrap<'_, Ext> {
    fn set_termination_reason(&mut self, reason: UndefinedTerminationReason) {
        self.host_state_mut().set_termination_reason(reason);
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
