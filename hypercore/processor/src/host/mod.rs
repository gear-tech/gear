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
use core_processor::common::JournalNote;
use gear_core::{code::InstrumentedCode, ids::ProgramId};
use gprimitives::{CodeId, H256};
use parity_scale_codec::{Decode, Encode};
use sp_allocator::{AllocationStats, FreeingBumpHeapAllocator};
use sp_wasm_interface::{HostState, IntoValue, MemoryWrapper, StoreData};
use std::{mem, sync::Arc};

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

#[derive(Clone)]
pub(crate) struct InstanceCreator {
    db: Database,
    engine: wasmtime::Engine,
    instance_pre: Arc<wasmtime::InstancePre<StoreData>>,

    /// Current chain head hash.
    ///
    /// NOTE: must be preset each time processor start to process new chain head.
    chain_head: Option<H256>,
}

impl InstanceCreator {
    pub fn new(db: Database, runtime: Vec<u8>) -> Result<Self> {
        gear_runtime_interface::sandbox_init();

        let engine = wasmtime::Engine::default();

        let module = wasmtime::Module::new(&engine, runtime)?;
        let mut linker = wasmtime::Linker::new(&engine);

        api::allocator::link(&mut linker)?;
        api::database::link(&mut linker)?;
        api::lazy_pages::link(&mut linker)?;
        api::logging::link(&mut linker)?;
        api::sandbox::link(&mut linker)?;

        let instance_pre = linker.instantiate_pre(&module)?;
        let instance_pre = Arc::new(instance_pre);

        Ok(Self {
            db,
            engine,
            instance_pre,
            chain_head: None,
        })
    }

    pub fn instantiate(&self) -> Result<InstanceWrapper> {
        let mut store = Store::new(&self.engine, Default::default());

        let instance = self.instance_pre.instantiate(&mut store)?;

        let mut instance_wrapper = InstanceWrapper {
            instance,
            store,
            db: self.db().clone(),
            chain_head: self.chain_head,
        };

        let memory = instance_wrapper.memory()?;
        let table = instance_wrapper.table()?;

        instance_wrapper.data_mut().memory = Some(memory);
        instance_wrapper.data_mut().table = Some(table);

        Ok(instance_wrapper)
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn set_chain_head(&mut self, chain_head: H256) {
        self.chain_head = Some(chain_head);
    }
}

pub(crate) struct InstanceWrapper {
    instance: wasmtime::Instance,
    store: Store,
    db: Database,
    chain_head: Option<H256>,
}

impl InstanceWrapper {
    pub fn db(&self) -> &Database {
        &self.db
    }

    #[allow(unused)]
    pub fn data(&self) -> &StoreData {
        self.store.data()
    }

    pub fn data_mut(&mut self) -> &mut StoreData {
        self.store.data_mut()
    }

    pub fn instrument(
        &mut self,
        original_code: impl AsRef<[u8]>,
    ) -> Result<Option<InstrumentedCode>> {
        self.call("instrument_code", original_code)
    }

    pub fn run(
        &mut self,
        program_id: ProgramId,
        original_code_id: CodeId,
        state_hash: H256,
        maybe_instrumented_code: Option<InstrumentedCode>,
    ) -> Result<Vec<JournalNote>> {
        let chain_head = self.chain_head.expect("chain head must be set before run");
        threads::set(self.db.clone(), chain_head, state_hash);

        let arg = (
            program_id,
            original_code_id,
            state_hash,
            maybe_instrumented_code,
        );

        self.call("run", arg.encode())
    }

    pub fn wake_messages(&mut self, program_id: ProgramId, state_hash: H256) -> Result<H256> {
        let chain_head = self.chain_head.expect("chain head must be set before wake");
        threads::set(self.db.clone(), chain_head, state_hash);

        self.call("wake_messages", (program_id, state_hash).encode())
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

    fn memory(&mut self) -> Result<wasmtime::Memory> {
        let memory_export = self
            .instance
            .get_export(&mut self.store, "memory")
            .ok_or_else(|| anyhow!("couldn't find `memory` export"))?;

        let memory = memory_export
            .into_memory()
            .ok_or_else(|| anyhow!("`memory` is not memory"))?;

        Ok(memory)
    }

    fn table(&mut self) -> Result<wasmtime::Table> {
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
