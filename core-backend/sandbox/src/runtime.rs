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

use crate::MemoryWrap;
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
use gear_backend_common::{
    memory::{
        MemoryAccessError, MemoryAccessManager, MemoryAccessRecorder, MemoryOwner, WasmMemoryRead,
        WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    runtime::{RunFallibleError, Runtime as CommonRuntime},
    BackendExternalities, BackendState, BackendTermination, UndefinedTerminationReason,
};
use gear_core::{costs::RuntimeCosts, pages::WasmPage};
use gear_sandbox::{HostError, InstanceGlobals, Value};
use gear_wasm_instrument::GLOBAL_NAME_GAS;

pub(crate) fn as_i64(v: Value) -> Option<i64> {
    match v {
        Value::I64(i) => Some(i),
        _ => None,
    }
}

pub(crate) fn as_u64(v: Value) -> Option<u64> {
    as_i64(v).map(|v| v as u64)
}

pub(crate) struct Runtime<Ext> {
    pub ext: Ext,
    pub memory: MemoryWrap,
    pub termination_reason: UndefinedTerminationReason,
    pub globals: gear_sandbox::default_executor::InstanceGlobals,
    // TODO: make wrapper around runtime and move memory_manager there (issue #2067)
    pub memory_manager: MemoryAccessManager<Ext>,
}

impl<Ext: BackendExternalities> CommonRuntime<Ext> for Runtime<Ext> {
    type Error = HostError;

    fn ext_mut(&mut self) -> &mut Ext {
        &mut self.ext
    }

    fn unreachable_error() -> Self::Error {
        HostError
    }

    fn run_any<T, F>(&mut self, cost: RuntimeCosts, f: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self) -> Result<T, UndefinedTerminationReason>,
    {
        self.with_globals_update(|ctx| {
            ctx.prepare_run();
            ctx.ext.charge_gas_runtime(cost)?;
            f(ctx)
        })
    }

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
        self.run_any(cost, |ctx| {
            let res = f(ctx);
            let res = ctx.process_fallible_func_result(res)?;

            // TODO: move above or make normal process memory access.
            let write_res = ctx.memory_manager.register_write_as::<R>(res_ptr);

            ctx.write_as(write_res, R::from(res)).map_err(Into::into)
        })
        .map(|_| ())
    }

    fn alloc(&mut self, pages: u32) -> Result<WasmPage, <Ext>::AllocError> {
        self.ext.alloc(pages, &mut self.memory)
    }
}

impl<Ext: BackendExternalities> Runtime<Ext> {
    // Cleans `memory_manager`, updates ext counters based on globals.
    fn prepare_run(&mut self) {
        self.memory_manager = Default::default();

        let gas = self
            .globals
            .get_global_val(GLOBAL_NAME_GAS)
            .and_then(as_u64)
            .unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        self.ext.decrease_current_counter_to(gas);
    }

    // Updates globals after execution.
    fn update_globals(&mut self) {
        let gas = self.ext.define_current_counter();

        self.globals
            .set_global_val(GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .unwrap_or_else(|e| {
                unreachable!("Globals must be checked during env creation: {:?}", e)
            });
    }

    fn with_globals_update<T, F>(&mut self, f: F) -> Result<T, HostError>
    where
        F: FnOnce(&mut Self) -> Result<T, UndefinedTerminationReason>,
    {
        let result = f(self).map_err(|err| {
            self.set_termination_reason(err);
            HostError
        });

        self.update_globals();

        result
    }

    fn with_memory<R, F>(&mut self, f: F) -> Result<R, MemoryAccessError>
    where
        F: FnOnce(
            &mut MemoryAccessManager<Ext>,
            &mut MemoryWrap,
            &mut u64,
        ) -> Result<R, MemoryAccessError>,
    {
        let mut gas_counter = self.ext.define_current_counter();

        // With memory ops do similar subtractions for both counters.
        let res = f(&mut self.memory_manager, &mut self.memory, &mut gas_counter);

        self.ext.decrease_current_counter_to(gas_counter);
        res
    }
}

impl<Ext: BackendExternalities> MemoryAccessRecorder for Runtime<Ext> {
    fn register_read(&mut self, ptr: u32, size: u32) -> WasmMemoryRead {
        self.memory_manager.register_read(ptr, size)
    }

    fn register_read_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryReadAs<T> {
        self.memory_manager.register_read_as(ptr)
    }

    fn register_read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        ptr: u32,
    ) -> WasmMemoryReadDecoded<T> {
        self.memory_manager.register_read_decoded(ptr)
    }

    fn register_write(&mut self, ptr: u32, size: u32) -> WasmMemoryWrite {
        self.memory_manager.register_write(ptr, size)
    }

    fn register_write_as<T: Sized>(&mut self, ptr: u32) -> WasmMemoryWriteAs<T> {
        self.memory_manager.register_write_as(ptr)
    }
}

impl<Ext: BackendExternalities> MemoryOwner for Runtime<Ext> {
    fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| manager.read(memory, read, gas_left))
    }

    fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        self.with_memory(move |manager, memory, gas_left| manager.read_as(memory, read, gas_left))
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

impl<Ext> BackendState for Runtime<Ext> {
    fn set_termination_reason(&mut self, reason: UndefinedTerminationReason) {
        self.termination_reason = reason;
    }
}

impl<Ext: BackendExternalities> BackendTermination<Ext, MemoryWrap> for Runtime<Ext> {
    fn into_parts(self) -> (Ext, MemoryWrap, UndefinedTerminationReason) {
        let Self {
            ext,
            memory,
            termination_reason,
            ..
        } = self;
        (ext, memory, termination_reason)
    }
}
