// This file is part of Gear.
//
// Copyright (C) 2024 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use anyhow::{anyhow, Result};
use gear_core::{code::InstrumentedCode, ids::ProgramId};
use gprimitives::H256;
use hypercore_runtime_common::state::Storage;
use hypercore_runtime_native::RuntimeInterface;
use parity_scale_codec::{Decode, Encode};
use sp_allocator::{AllocationStats, FreeingBumpHeapAllocator};
use sp_wasm_interface::{HostState, IntoValue, MemoryWrapper, StoreData};
use std::mem;
use wasmtime::{Memory, Table};

use crate::Database;

pub mod api;
pub mod runtime;

mod context;
mod threads;

pub fn runtime() -> Vec<u8> {
    let mut runtime = runtime::Runtime::new();
    runtime.add_start_section();
    runtime.into_bytes()
}

pub type Store = wasmtime::Store<StoreData>;

pub struct InstanceWrapper {
    pub instance: wasmtime::Instance,
    pub store: Store,
    pub db: Database,
}

impl InstanceWrapper {
    pub fn data(&self) -> &StoreData {
        self.store.data()
    }

    pub fn data_mut(&mut self) -> &mut StoreData {
        self.store.data_mut()
    }

    pub fn new(db: Database) -> Result<Self> {
        gear_runtime_interface::sandbox_init();

        let mut store = Store::default();
        let module = wasmtime::Module::new(store.engine(), runtime())?;
        let mut linker = wasmtime::Linker::new(store.engine());

        api::allocator::link(&mut linker)?;
        api::database::link(&mut linker)?;
        api::lazy_pages::link(&mut linker)?;
        api::logging::link(&mut linker)?;
        api::sandbox::link(&mut linker)?;

        let instance = linker.instantiate(&mut store, &module)?;
        let mut instance_wrapper = Self {
            instance,
            store,
            db,
        };

        let memory = instance_wrapper.memory()?;
        let table = instance_wrapper.table()?;

        instance_wrapper.data_mut().memory = Some(memory);
        instance_wrapper.data_mut().table = Some(table);

        Ok(instance_wrapper)
    }

    pub fn instrument(&mut self, original_code: &Vec<u8>) -> Result<Option<InstrumentedCode>> {
        self.call("instrument", original_code)
    }

    pub fn run(&mut self, state_hash: H256, instrumented_code: &InstrumentedCode) -> Result<()> {
        threads::set(self.db.clone(), state_hash);

        self.call("run", instrumented_code.encode())
    }

    pub fn verify(&mut self, code: &Vec<u8>) -> Result<bool> {
        self.call("verify", code)
    }

    fn call<D: Decode>(&mut self, name: &'static str, input: impl AsRef<[u8]>) -> Result<D> {
        self.with_host_state(|instance_wrapper| {
            let func = instance_wrapper
                .instance
                .get_typed_func::<(i32, i32), i64>(&mut instance_wrapper.store, name)?;

            let input_data = instance_wrapper.set_call_input(input.as_ref())?;

            let output_ptr_len = func.call(&mut instance_wrapper.store, input_data)?;

            let output = instance_wrapper.get_call_output(output_ptr_len)?;

            Ok(output)
        })
    }

    fn with_host_state<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<T> {
        self.set_host_state()?;
        let res = f(self);
        let _allocation_stats = self.reset_host_state()?;
        res
    }

    fn set_call_input(&mut self, bytes: &[u8]) -> Result<(i32, i32)> {
        let memory = self.memory()?;

        let len = bytes.len() as u32; // TODO: check len.

        let ptr = self.with_allocator(|instance_wrapper, allocator| {
            allocator
                .allocate(
                    &mut MemoryWrapper::from((&memory, &mut instance_wrapper.store)),
                    len,
                )
                .map_err(Into::into)
        })?;

        sp_wasm_interface::util::write_memory_from(&mut self.store, ptr, bytes)
            .map_err(|e| anyhow!("failed to write call input: {e}"))?;

        let ptr = ptr.into_value().as_i32().expect("must be i32");

        Ok((ptr, len as i32))
    }

    fn get_call_output<D: Decode>(&mut self, ptr_len: i64) -> Result<D> {
        let [ptr, len]: [i32; 2] = unsafe { mem::transmute(ptr_len) };

        // TODO: check range.
        let memory = self.memory()?;
        let mut res = &memory.data(&self.store)[ptr as usize..(ptr + len) as usize];

        let res = D::decode(&mut res)?;

        Ok(res)
    }

    fn set_host_state(&mut self) -> Result<()> {
        let heap_base = self.heap_base()?;

        let allocator = FreeingBumpHeapAllocator::new(heap_base);

        let host_state = HostState::new(allocator);

        self.data_mut().host_state = Some(host_state);

        Ok(())
    }

    fn reset_host_state(&mut self) -> Result<AllocationStats> {
        let host_state = self
            .data_mut()
            .host_state
            .take()
            .ok_or_else(|| anyhow!("host state should be set before call and reset after"))?;

        Ok(host_state.allocation_stats())
    }

    fn with_allocator<T>(
        &mut self,
        f: impl FnOnce(&mut Self, &mut FreeingBumpHeapAllocator) -> Result<T>,
    ) -> Result<T> {
        let mut allocator = self
            .data_mut()
            .host_state
            .as_mut()
            .and_then(|s| s.allocator.take())
            .ok_or_else(|| anyhow!("allocator should be set after `set_host_state`"))?;

        let res = f(self, &mut allocator);

        self.data_mut()
            .host_state
            .as_mut()
            .expect("checked above")
            .allocator = Some(allocator);

        res
    }

    fn memory(&mut self) -> Result<Memory> {
        let memory_export = self
            .instance
            .get_export(&mut self.store, "memory")
            .ok_or_else(|| anyhow!("couldn't find `memory` export"))?;

        let memory = memory_export
            .into_memory()
            .ok_or_else(|| anyhow!("`memory` is not memory"))?;

        Ok(memory)
    }

    fn table(&mut self) -> Result<Table> {
        let table_export = self
            .instance
            .get_export(&mut self.store, "__indirect_function_table")
            .ok_or_else(|| anyhow!("couldn't find `__indirect_function_table` export"))?;

        let table = table_export
            .into_table()
            .ok_or_else(|| anyhow!("`__indirect_function_table` is not table"))?;

        Ok(table)
    }

    fn heap_base(&mut self) -> Result<u32> {
        let heap_base_export = self
            .instance
            .get_export(&mut self.store, "__heap_base")
            .ok_or_else(|| anyhow!("couldn't find `__heap_base` export"))?;

        let heap_base_global = heap_base_export
            .into_global()
            .ok_or_else(|| anyhow!("`__heap_base` is not global"))?;

        let heap_base = heap_base_global
            .get(&mut self.store)
            .i32()
            .ok_or_else(|| anyhow!("`__heap_base` is not i32"))?;

        Ok(heap_base as u32)
    }
}
