// This file is part of Gear.

// Copyright (C) 2021-2024 Gear Technologies Inc.
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

//! sp-sandbox runtime (here it's program execution state) realization.

use crate::{
    error::{BackendAllocSyscallError, RunFallibleError, UndefinedTerminationReason},
    memory::{ExecutorMemory, MemoryAccessRegistrar, MemoryWrapRef},
    state::{HostState, State},
    BackendExternalities,
};
use gear_core::{costs::CostToken, pages::WasmPage};
use gear_sandbox::{default_executor::Caller, AsContextExt, HostError, Value};

pub(crate) fn as_i64(v: Value) -> Option<i64> {
    match v {
        Value::I64(i) => Some(i),
        _ => None,
    }
}

#[track_caller]
pub(crate) fn caller_host_state_mut<'a, 'b: 'a, Ext>(
    caller: &'a mut Caller<'_, HostState<Ext, ExecutorMemory>>,
) -> &'a mut State<Ext, ExecutorMemory> {
    caller
        .data_mut()
        .as_mut()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

#[track_caller]
pub(crate) fn caller_host_state_take<Ext>(
    caller: &mut Caller<'_, HostState<Ext, ExecutorMemory>>,
) -> State<Ext, ExecutorMemory> {
    caller
        .data_mut()
        .take()
        .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
}

pub(crate) struct CallerWrap<'a, 'b: 'a, Ext> {
    pub caller: &'a mut Caller<'b, HostState<Ext, ExecutorMemory>>,
    pub memory: ExecutorMemory,
}

impl<'a, 'b, Ext: BackendExternalities + 'static> CallerWrap<'a, 'b, Ext> {
    pub fn ext_mut(&mut self) -> &mut Ext {
        &mut self.host_state_mut().ext
    }

    #[track_caller]
    pub fn run_any<T, F>(&mut self, gas: u64, token: CostToken, f: F) -> Result<(u64, T), HostError>
    where
        F: FnOnce(&mut Self) -> Result<T, UndefinedTerminationReason>,
    {
        self.host_state_mut().ext.decrease_current_counter_to(gas);

        let run = || {
            self.host_state_mut().ext.charge_gas_for_token(token)?;
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
        token: CostToken,
        f: F,
    ) -> Result<(u64, ()), HostError>
    where
        F: FnOnce(&mut Self) -> Result<T, RunFallibleError>,
        R: From<Result<T, u32>> + Sized,
    {
        self.run_any(
            gas,
            token,
            |ctx: &mut Self| -> Result<_, UndefinedTerminationReason> {
                let res = f(ctx);
                let res = ctx.process_fallible_func_result(res)?;

                // TODO: move above or make normal process memory access.
                let mut registrar = MemoryAccessRegistrar::default();
                let write_res = registrar.register_write_as::<R>(res_ptr);
                let mut io = registrar.pre_process(ctx)?;
                io.write_as(write_res, R::from(res)).map_err(Into::into)
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
    pub fn prepare(caller: &'a mut Caller<'b, HostState<Ext, ExecutorMemory>>) -> Self {
        let memory = caller_host_state_mut(caller).memory.clone();
        Self { caller, memory }
    }

    #[track_caller]
    pub fn host_state_mut(&mut self) -> &mut State<Ext, ExecutorMemory> {
        caller_host_state_mut(self.caller)
    }

    #[track_caller]
    pub fn memory<'c, 'd: 'c>(
        caller: &'c mut Caller<'d, HostState<Ext, ExecutorMemory>>,
        memory: ExecutorMemory,
    ) -> MemoryWrapRef<'c, 'd, Ext> {
        MemoryWrapRef::<'c, 'd, _> { memory, caller }
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
