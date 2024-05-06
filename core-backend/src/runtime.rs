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
    memory::{ExecutorMemory, MemoryAccessRegistry, MemoryWrapRef},
    state::{HostState, State},
    BackendExternalities,
};
use gear_core::{costs::CostToken, pages::WasmPage};
use gear_sandbox::{AsContextExt, HostError};

pub(crate) struct CallerWrap<'a, Caller> {
    pub caller: &'a mut Caller,
}

impl<'a, Caller, Ext, Mem> CallerWrap<'a, Caller>
where
    Caller: AsContextExt<State = HostState<Ext, Mem>>,
    Mem: Clone + 'static,
{
    pub fn new(caller: &'a mut Caller) -> Self {
        Self { caller }
    }

    #[track_caller]
    pub fn state_mut(&mut self) -> &mut State<Ext, Mem> {
        self.caller
            .data_mut()
            .as_mut()
            .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
    }

    #[track_caller]
    pub fn take_state(&mut self) -> State<Ext, Mem> {
        self.caller
            .data_mut()
            .take()
            .unwrap_or_else(|| unreachable!("host_state must be set before execution"))
    }

    pub fn set_termination_reason(&mut self, reason: UndefinedTerminationReason) {
        self.state_mut().termination_reason = reason;
    }

    pub fn ext_mut(&mut self) -> &mut Ext {
        &mut self.state_mut().ext
    }
}

impl<'a, Caller, Ext> CallerWrap<'a, Caller>
where
    Caller: AsContextExt<State = HostState<Ext, ExecutorMemory>>,
    Ext: BackendExternalities + 'static,
{
    #[track_caller]
    pub fn run_any<U, F>(&mut self, gas: u64, token: CostToken, f: F) -> Result<(u64, U), HostError>
    where
        F: FnOnce(&mut Self) -> Result<U, UndefinedTerminationReason>,
    {
        self.state_mut().ext.decrease_current_counter_to(gas);

        let run = || {
            self.state_mut().ext.charge_gas_for_token(token)?;
            f(self)
        };

        run()
            .map_err(|err| {
                self.set_termination_reason(err);
                HostError
            })
            .map(|r| (self.state_mut().ext.define_current_counter(), r))
    }

    #[track_caller]
    pub fn run_fallible<U: Sized, F, R>(
        &mut self,
        gas: u64,
        res_ptr: u32,
        token: CostToken,
        f: F,
    ) -> Result<(u64, ()), HostError>
    where
        F: FnOnce(&mut Self) -> Result<U, RunFallibleError>,
        R: From<Result<U, u32>> + Sized,
    {
        self.run_any(
            gas,
            token,
            |ctx: &mut Self| -> Result<_, UndefinedTerminationReason> {
                let res = f(ctx);
                let res = ctx.process_fallible_func_result(res)?;

                // TODO: move above or make normal process memory access.
                let mut registry = MemoryAccessRegistry::default();
                let write_res = registry.register_write_as::<R>(res_ptr);
                let mut io = registry.pre_process(ctx)?;
                io.write_as(write_res, R::from(res)).map_err(Into::into)
            },
        )
    }

    pub fn alloc(&mut self, pages: u32) -> Result<WasmPage, <Ext>::AllocError> {
        let mut state = self.take_state();
        let memory = state.memory.clone();
        let mut memory = MemoryWrapRef {
            memory,
            caller: self.caller,
        };

        let res = state.ext.alloc(pages, &mut memory);
        self.caller.data_mut().replace(state);
        res
    }

    /// Process fallible syscall function result
    pub fn process_fallible_func_result<U: Sized>(
        &mut self,
        res: Result<U, RunFallibleError>,
    ) -> Result<Result<U, u32>, UndefinedTerminationReason> {
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
    pub fn process_alloc_func_result<U: Sized, ExtAllocError: BackendAllocSyscallError>(
        &mut self,
        res: Result<U, ExtAllocError>,
    ) -> Result<Result<U, ExtAllocError>, UndefinedTerminationReason> {
        match res {
            Ok(t) => Ok(Ok(t)),
            Err(err) => match err.into_backend_error() {
                Ok(ext_err) => Err(ext_err.into()),
                Err(alloc_err) => Ok(Err(alloc_err)),
            },
        }
    }
}
