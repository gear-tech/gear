// This file is part of Gear.
//
// Copyright (C) 2024-2025 Gear Technologies Inc.
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

use crate::{Database, ProcessorError, Result};
use core_processor::common::JournalNote;
use ethexe_common::gear::Origin;
use ethexe_runtime_common::{ProgramJournals, unpack_i64_to_u32};
use gear_core::{
    code::{CodeMetadata, InstrumentedCode},
    ids::ActorId,
};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use sp_allocator::{AllocationStats, FreeingBumpHeapAllocator};
use sp_wasm_interface::{HostState, IntoValue, MemoryWrapper, StoreData};
use std::sync::Arc;

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
    engine: wasmtime::Engine,
    instance_pre: Arc<wasmtime::InstancePre<StoreData>>,

    /// Current chain head hash.
    ///
    /// NOTE: must be preset each time processor start to process new chain head.
    chain_head: Option<H256>,
}

impl InstanceCreator {
    pub fn new(runtime: Vec<u8>) -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.cache_config_load_default()?;
        let engine = wasmtime::Engine::new(&config)?;

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
            chain_head: self.chain_head,
        };

        let memory = instance_wrapper.memory()?;
        let table = instance_wrapper.table()?;

        instance_wrapper.data_mut().memory = Some(memory);
        instance_wrapper.data_mut().table = Some(table);

        Ok(instance_wrapper)
    }

    pub fn set_chain_head(&mut self, chain_head: H256) {
        self.chain_head = Some(chain_head);
    }
}

pub(crate) struct InstanceWrapper {
    instance: wasmtime::Instance,
    store: Store,
    chain_head: Option<H256>,
}

impl InstanceWrapper {
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
    ) -> Result<Option<(InstrumentedCode, CodeMetadata)>> {
        self.call("instrument_code", original_code)
    }

    pub fn run(
        &mut self,
        db: Database,
        program_id: ActorId,
        state_hash: H256,
        maybe_instrumented_code: Option<InstrumentedCode>,
        maybe_code_metadata: Option<CodeMetadata>,
        gas_allowance: u64,
    ) -> Result<(ProgramJournals, H256, u64)> {
        let chain_head = self.chain_head.expect("chain head must be set before run");
        threads::set(db, chain_head, state_hash);

        let arg = (
            program_id,
            state_hash,
            maybe_instrumented_code,
            maybe_code_metadata,
            gas_allowance,
        );

        // Pieces of resulting journal. Hack to avoid single allocation limit.
        let (ptr_lens, gas_spent): (Vec<i64>, i64) = self.call("run", arg.encode())?;

        let mut mega_journal = Vec::with_capacity(ptr_lens.len());

        for ptr_len in ptr_lens {
            let journal_and_origin: (Vec<JournalNote>, Origin, bool) =
                self.get_call_output(ptr_len)?;
            mega_journal.push(journal_and_origin);
        }

        let new_state_hash = threads::with_params(|params| params.state_hash);

        Ok((mega_journal, new_state_hash, gas_spent as u64))
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
            .map_err(ProcessorError::CallInputWrite)?;

        let ptr = ptr.into_value().as_i32().expect("must be i32");

        Ok((ptr, len as i32))
    }

    fn get_call_output<D: Decode>(&mut self, ptr_len: i64) -> Result<D> {
        let (ptr, len) = unpack_i64_to_u32(ptr_len);

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
            .ok_or(ProcessorError::HostStateNotSet)?;

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
            .ok_or(ProcessorError::AllocatorNotSet)?;

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
            .ok_or(ProcessorError::MemoryExportNotFound)?;

        let memory = memory_export
            .into_memory()
            .ok_or(ProcessorError::InvalidMemory)?;

        Ok(memory)
    }

    fn table(&mut self) -> Result<wasmtime::Table> {
        let table_export = self
            .instance
            .get_export(&mut self.store, "__indirect_function_table")
            .ok_or(ProcessorError::IndirectFunctionTableNotFound)?;

        let table = table_export
            .into_table()
            .ok_or(ProcessorError::InvalidIndirectFunctionTable)?;

        Ok(table)
    }

    fn heap_base(&mut self) -> Result<u32> {
        let heap_base_export = self
            .instance
            .get_export(&mut self.store, "__heap_base")
            .ok_or(ProcessorError::HeapBaseNotFound)?;

        let heap_base_global = heap_base_export
            .into_global()
            .ok_or(ProcessorError::HeapBaseIsNotGlobal)?;

        let heap_base = heap_base_global
            .get(&mut self.store)
            .i32()
            .ok_or(ProcessorError::HeapBaseIsNoti32)?;

        Ok(heap_base as u32)
    }
}
