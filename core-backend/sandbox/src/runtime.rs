// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use crate::{funcs::SyscallOutput, MemoryWrap};
use alloc::vec::Vec;
use codec::{Decode, MaxEncodedLen};
use gear_backend_common::{
    memory::{
        MemoryAccessError, MemoryAccessManager, MemoryAccessRecorder, MemoryOwner, WasmMemoryRead,
        WasmMemoryReadAs, WasmMemoryReadDecoded, WasmMemoryWrite, WasmMemoryWriteAs,
    },
    BackendExt, BackendExtError, BackendState, SyscallFuncError,
};
use gear_core::env::Ext;
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use sp_sandbox::{HostError, InstanceGlobals, ReturnValue, Value};

pub(crate) fn as_i64(v: Value) -> Option<i64> {
    match v {
        Value::I64(i) => Some(i),
        _ => None,
    }
}

pub(crate) struct Runtime<E: Ext> {
    pub ext: E,
    pub memory: MemoryWrap,
    pub err: SyscallFuncError<E::Error>,
    pub globals: sp_sandbox::default_executor::InstanceGlobals,
    // TODO: make wrapper around runtime and move memory_manager there (issue #2067)
    pub memory_manager: MemoryAccessManager<E>,
}

impl<E> Runtime<E>
where
    E: Ext,
    E::Error: BackendExtError,
{
    pub(crate) fn run_any<T, F>(&mut self, f: F) -> Result<T, HostError>
    where
        F: FnOnce(&mut Self) -> Result<T, SyscallFuncError<E::Error>>,
    {
        self.memory_manager = Default::default();

        let gas = self
            .globals
            .get_global_val(GLOBAL_NAME_GAS)
            .and_then(as_i64)
            .unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        let allowance = self
            .globals
            .get_global_val(GLOBAL_NAME_ALLOWANCE)
            .and_then(as_i64)
            .unwrap_or_else(|| unreachable!("Globals must be checked during env creation"));

        self.ext.update_counters(gas as u64, allowance as u64);

        let result = f(self).map_err(|err| {
            self.err = err;
            HostError
        });

        let (gas, allowance) = self.ext.counters();

        self.globals
            .set_global_val(GLOBAL_NAME_GAS, Value::I64(gas as i64))
            .unwrap_or_else(|e| {
                unreachable!("Globals must be checked during env creation: {:?}", e)
            });

        self.globals
            .set_global_val(GLOBAL_NAME_ALLOWANCE, Value::I64(allowance as i64))
            .unwrap_or_else(|e| {
                unreachable!("Globals must be checked during env creation: {:?}", e)
            });

        result
    }

    pub(crate) fn run<F>(&mut self, f: F) -> SyscallOutput
    where
        F: FnOnce(&mut Self) -> Result<(), SyscallFuncError<E::Error>>,
    {
        self.run_any(f).map(|_| ReturnValue::Unit)
    }
}

impl<E> BackendState<E::Error> for Runtime<E>
where
    E: Ext,
    E::Error: BackendExtError,
{
    fn err_mut(&mut self) -> &mut SyscallFuncError<E::Error> {
        &mut self.err
    }
}

impl<E: Ext> MemoryAccessRecorder for Runtime<E> {
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

impl<E> MemoryOwner for Runtime<E>
where
    E: BackendExt,
{
    fn read(&mut self, read: WasmMemoryRead) -> Result<Vec<u8>, MemoryAccessError> {
        self.memory_manager.read(&self.memory, read)
    }

    fn read_as<T: Sized>(&mut self, read: WasmMemoryReadAs<T>) -> Result<T, MemoryAccessError> {
        self.memory_manager.read_as(&self.memory, read)
    }

    fn read_decoded<T: Decode + MaxEncodedLen>(
        &mut self,
        read: WasmMemoryReadDecoded<T>,
    ) -> Result<T, MemoryAccessError> {
        self.memory_manager.read_decoded(&self.memory, read)
    }

    fn write(&mut self, write: WasmMemoryWrite, buff: &[u8]) -> Result<(), MemoryAccessError> {
        self.memory_manager.write(&mut self.memory, write, buff)
    }

    fn write_as<T: Sized>(
        &mut self,
        write: WasmMemoryWriteAs<T>,
        obj: T,
    ) -> Result<(), MemoryAccessError> {
        self.memory_manager.write_as(&mut self.memory, write, obj)
    }
}
