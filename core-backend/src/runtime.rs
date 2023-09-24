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
    memory::{
        MemoryAccessError, MemoryAccessManager, MemoryWrapRef, WasmMemoryRead, WasmMemoryReadAs,
        WasmMemoryReadDecoded, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    state::{HostState, State},
    BackendAllocSyscallError, BackendExternalities, DefaultExecutorMemory, RunFallibleError,
    UndefinedTerminationReason,
};
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
use gear_core::{costs::RuntimeCosts, pages::WasmPage};
use gear_sandbox::{default_executor::Caller, AsContextExt, HostError, Value};

pub(crate) fn as_i64(v: Value) -> Option<i64> {
    match v {
        Value::I64(i) => Some(i),
        _ => None,
    }
}

#[track_caller]
pub(crate) fn caller_host_state_mut<'a, 'b: 'a, Ext>(
    caller: &'a mut Caller<'_, HostState<Ext, DefaultExecutorMemory>>,
) -> &'a mut State<Ext, DefaultExecutorMemory> {
    caller
        .data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

#[track_caller]
pub(crate) fn caller_host_state_take<Ext>(
    caller: &mut Caller<'_, HostState<Ext, DefaultExecutorMemory>>,
) -> State<Ext, DefaultExecutorMemory> {
    caller
        .data_mut()
        .take()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub struct CallerWrap<'a, 'b: 'a, Ext> {
    pub caller: &'a mut Caller<'b, HostState<Ext, DefaultExecutorMemory>>,
    pub manager: MemoryAccessManager<Ext>,
    pub memory: DefaultExecutorMemory,
}

impl<'a, 'b, Ext: BackendExternalities + 'static> CallerWrap<'a, 'b, Ext> {
    pub fn ext_mut(&mut self) -> &mut Ext {
        &mut self.host_state_mut().ext
    }

    #[track_caller]
    pub fn run_any<T, F>(
        &mut self,
        gas: u64,
        cost: RuntimeCosts,
        f: F,
    ) -> Result<(u64, T), HostError>
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
    pub fn run_fallible<T: Sized, F, R>(
        &mut self,
        gas: u64,
        res_ptr: u32,
        cost: RuntimeCosts,
        f: F,
    ) -> Result<(u64, ()), HostError>
    where
        F: FnOnce(&mut Self) -> Result<T, RunFallibleError>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run_any(
            gas,
            cost,
            |ctx: &mut Self| -> Result<_, UndefinedTerminationReason> {
                let res = f(ctx);
                let res = ctx.process_fallible_func_result(res)?;

                // TODO: move above or make normal process memory access.
                let write_res = ctx.manager.register_write_as::<R>(res_ptr);

                ctx.write_as(write_res, R::from(res)).map_err(Into::into)
            },
        )
    }

    pub fn alloc(&mut self, pages: u32) -> Result<WasmPage, <Ext>::AllocError> {
        let mut state = caller_host_state_take(self.caller);
        let mut mem = CallerWrap::memory(self.caller, self.memory.clone());
        let res = state.ext.alloc(pages, &mut mem);
        self.caller.data_mut().replace(state);
        res
    }

    #[track_caller]
    pub fn prepare(
        caller: &'a mut Caller<'b, HostState<Ext, DefaultExecutorMemory>>,
    ) -> Result<Self, HostError> {
        let memory = caller_host_state_mut(caller).memory.clone();
        Ok(Self {
            caller,
            manager: Default::default(),
            memory,
        })
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<Ext, DefaultExecutorMemory> {
        caller_host_state_mut(self.caller)
    }

    #[track_caller]
    pub fn memory<'c, 'd: 'c>(
        caller: &'c mut Caller<'d, HostState<Ext, DefaultExecutorMemory>>,
        memory: DefaultExecutorMemory,
    ) -> MemoryWrapRef<'c, 'd, Ext> {
        MemoryWrapRef::<'c, 'd, _> { memory, caller }
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

        let mut memory = Self::memory(self.caller, self.memory.clone());

        // With memory ops do similar subtractions for both counters.
        let res = f(&mut self.manager, &mut memory, &mut gas_counter);

        self.host_state_mut()
            .ext
            .decrease_current_counter_to(gas_counter);
        res
    }

    pub fn set_termination_reason(&mut self, reason: UndefinedTerminationReason) {
        self.host_state_mut().termination_reason = reason;
    }

    /// Process fallible syscall function result
    pub fn process_fallible_func_result<T: Sized>(
        &mut self,
        res: Result<T, RunFallibleError>,
    ) -> Result<Result<T, u32>, UndefinedTerminationReason> {
        match res {
            Err(RunFallibleError::FallibleExt(ext_err)) => {
                let code = ext_err.to_u32();
                log::trace!(target: "syscalls", "fallible syscall error: {ext_err}");
                Ok(Err(code))
            }
            Err(RunFallibleError::UndefinedTerminationReason(reason)) => Err(reason),
            Ok(res) => Ok(Ok(res)),
        }
    }

    /// Process alloc function result
    pub fn process_alloc_func_result<T: Sized, ExtAllocError: BackendAllocSyscallError>(
        &mut self,
        res: Result<T, ExtAllocError>,
    ) -> Result<Result<T, ExtAllocError>, UndefinedTerminationReason> {
        match res {
            Ok(t) => Ok(Ok(t)),
            Err(err) => match err.into_backend_error() {
                Ok(ext_err) => Err(ext_err.into()),
                Err(alloc_err) => Ok(Err(alloc_err)),
            },
        }
    }
}

impl<'a, 'b, Ext: BackendExternalities + 'static> CallerWrap<'a, 'b, Ext> {
    pub fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        self.with_memory(|manager, memory, gas_left| manager.read(memory, read, gas_left))
    }

    pub fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        self.with_memory(|manager, memory, gas_left| manager.read_as(memory, read, gas_left))
    }

    pub fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.read_decoded(memory, read, gas_left)
        })
    }

    pub fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.write(memory, write, buff, gas_left)
        })
    }

    pub fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| {
            manager.write_as(memory, write, obj, gas_left)
        })
    }
}
