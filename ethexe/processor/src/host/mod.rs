// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

use crate::BoundPromiseSink;
use core_processor::common::JournalNote;
use ethexe_common::gear::MessageType;
use ethexe_db::Database;
use ethexe_runtime_common::{ProcessQueueContext, ProgramJournals, unpack_i64_to_u32};
use gear_core::code::{CodeMetadata, InstrumentedCode};
use gprimitives::H256;
use parity_scale_codec::{Decode, Encode};
use sp_allocator::FreeingBumpHeapAllocator;
use std::sync::Arc;

pub mod api;
pub mod runtime;

mod context;
mod threads;

pub(crate) use context::StoreData;

#[derive(thiserror::Error, Debug)]
pub enum InstanceError {
    #[error("failed to write call input: out of bounds")]
    CallInputWriteOutOfBounds,
    #[error("couldn't find 'memory' export")]
    MemoryExportNotFound,
    #[error("'memory' export is not a wasm memory")]
    InvalidMemory,
    #[error("couldn't find `__indirect_function_table` export")]
    IndirectFunctionTableNotFound,
    #[error("`__indirect_function_table` is not table")]
    InvalidIndirectFunctionTable,
    #[error("couldn't find `__heap_base` export")]
    HeapBaseNotFound,
    #[error("`__heap_base` is not global")]
    HeapBaseIsNotGlobal,
    #[error("`__heap_base` is not i32")]
    HeapBaseIsNotI32,
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),
    #[error("decoding runtime call output error: {0}")]
    CallOutput(#[from] parity_scale_codec::Error),
    #[error("sp allocator error: {0}")]
    SpAllocator(#[from] sp_allocator::Error),
}

pub(super) type Result<T, E = InstanceError> = std::result::Result<T, E>;

/// Returns wasm runtime bytes.
///
/// The returned runtime is able to perform some functions
/// related to executing programs in the context of the gear protocol.
/// These functions are:
/// - `instrument_code` - instrument the code of the program.
/// - `run` - execute messages of the program in the context of the gear protocol.
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
}

impl InstanceCreator {
    /// Instantiates a wasm runtime instance creator.
    ///
    /// A wasm runtime here is a runtime for executing wasm programs
    /// in the context of the gear protocol programs execution.
    /// That actually brings some requirements for the wasm module
    /// instantiation, like linking expected host functions to use
    /// lazy pages, allocator or have an access to database.
    ///
    /// A wasm runtime modules is expected to use some runtime interface,
    /// which calls linked host functions.
    pub fn new(db: Database, runtime: Vec<u8>) -> Result<Self> {
        let mut config = wasmtime::Config::new();
        let cache = wasmtime::Cache::new(wasmtime::CacheConfig::default())?;
        config
            .cache(Some(cache))
            // Lazy-pages requires Wasmtime to use Unix signal handlers on
            // macOS, because Gear installs and chains SIGSEGV handlers.
            .macos_use_mach_ports(false);
        let engine = wasmtime::Engine::new(&config)?;

        let module = wasmtime::Module::new(&engine, runtime)?;
        let mut linker = wasmtime::Linker::new(&engine);

        api::allocator::link(&mut linker)?;
        api::database::link(&mut linker)?;
        api::lazy_pages::link(&mut linker)?;
        api::logging::link(&mut linker)?;
        api::sandbox::link(&mut linker)?;
        api::promise::link(&mut linker)?;

        let instance_pre = linker.instantiate_pre(&module)?;
        let instance_pre = Arc::new(instance_pre);

        Ok(Self {
            db,
            engine,
            instance_pre,
        })
    }

    pub fn instantiate(&self) -> Result<InstanceWrapper> {
        let store = StoreData {
            memory: None,
            table: None,
            allocator: None,
            db: self.db.cas().clone_boxed(),
            promise_sink: None,
        };
        let mut store = Store::new(&self.engine, store);

        let instance = self.instance_pre.instantiate(&mut store)?;

        let mut instance_wrapper = InstanceWrapper { instance, store };

        let memory = instance_wrapper.memory()?;
        let table = instance_wrapper.table()?;

        instance_wrapper.data_mut().memory = Some(memory);
        instance_wrapper.data_mut().table = Some(table);

        Ok(instance_wrapper)
    }

    pub(crate) fn with_db(&self, db: Database) -> Self {
        Self {
            db,
            engine: self.engine.clone(),
            instance_pre: self.instance_pre.clone(),
        }
    }
}

pub(crate) struct InstanceWrapper {
    instance: wasmtime::Instance,
    store: Store,
}

impl InstanceWrapper {
    pub fn data(&self) -> &StoreData {
        self.store.data()
    }

    pub fn data_mut(&mut self) -> &mut StoreData {
        self.store.data_mut()
    }

    /// Call to the exported `instrument_code` function of the wasm module.
    pub fn instrument(
        &mut self,
        original_code: impl AsRef<[u8]>,
    ) -> Result<Option<(InstrumentedCode, CodeMetadata)>> {
        self.call("instrument_code", original_code)
    }

    /// Call to the exported `run` function of the wasm module.
    ///
    /// The `run` function actually executed program's queue in accordance to
    /// the gear protocol. The returned sequence of `JournalNote`s is later
    /// processed out of the wasm module.
    pub fn run(
        &mut self,
        ctx: ProcessQueueContext,
        promise_sink: Option<BoundPromiseSink>,
    ) -> Result<(ProgramJournals, H256, u64)> {
        threads::set(self.data().db.clone_boxed(), ctx.state_root);

        self.with_promise_sink(promise_sink, |instance_wrapper| {
            // Pieces of resulting journal. Hack to avoid single allocation limit.
            let (ptr_lens, gas_spent): (Vec<i64>, i64) =
                instance_wrapper.call("run", ctx.encode())?;

            let mut mega_journal = Vec::with_capacity(ptr_lens.len());

            for ptr_len in ptr_lens {
                let journal_and_message_type: (Vec<JournalNote>, MessageType, bool) =
                    instance_wrapper.get_call_output(ptr_len)?;
                mega_journal.push(journal_and_message_type);
            }

            let new_state_hash = threads::state_hash();

            Ok((mega_journal, new_state_hash, gas_spent as u64))
        })
    }

    /// Low-level call to exported from the wasm module `name` function.
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
        let heap_base = self.heap_base()?;
        self.data_mut().allocator = Some(FreeingBumpHeapAllocator::new(heap_base));

        let res = f(self);

        let _allocator = self.data_mut().allocator.take().expect("allocator is None");

        res
    }

    fn with_promise_sink<T>(
        &mut self,
        promise_sink: Option<BoundPromiseSink>,
        f: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.data_mut().promise_sink = promise_sink;
        let res = f(self);
        let _ = self.data_mut().promise_sink.take();
        res
    }

    fn set_call_input(&mut self, bytes: &[u8]) -> Result<(i32, i32)> {
        let len = bytes.len() as u32; // TODO: check len.

        let ptr = context::allocator(&mut self.store).allocate(len)?;

        context::memory(&mut self.store)
            .slice_mut(ptr, len)
            .ok_or(InstanceError::CallInputWriteOutOfBounds)?
            .copy_from_slice(bytes);

        Ok((ptr as i32, len as i32))
    }

    fn get_call_output<D: Decode>(&mut self, ptr_len: i64) -> Result<D> {
        let (ptr, len) = unpack_i64_to_u32(ptr_len);

        // TODO: check range.
        let memory = self.memory()?;
        let mut res = &memory.data(&self.store)[ptr as usize..(ptr + len) as usize];

        let res = D::decode(&mut res)?;

        Ok(res)
    }

    fn memory(&mut self) -> Result<wasmtime::Memory> {
        let memory_export = self
            .instance
            .get_export(&mut self.store, "memory")
            .ok_or(InstanceError::MemoryExportNotFound)?;

        let memory = memory_export
            .into_memory()
            .ok_or(InstanceError::InvalidMemory)?;
        Ok(memory)
    }

    fn table(&mut self) -> Result<wasmtime::Table> {
        let table_export = self
            .instance
            .get_export(&mut self.store, "__indirect_function_table")
            .ok_or(InstanceError::IndirectFunctionTableNotFound)?;

        let table = table_export
            .into_table()
            .ok_or(InstanceError::InvalidIndirectFunctionTable)?;
        Ok(table)
    }

    fn heap_base(&mut self) -> Result<u32> {
        let heap_base_export = self
            .instance
            .get_export(&mut self.store, "__heap_base")
            .ok_or(InstanceError::HeapBaseNotFound)?;

        let heap_base_global = heap_base_export
            .into_global()
            .ok_or(InstanceError::HeapBaseIsNotGlobal)?;
        let heap_base = heap_base_global
            .get(&mut self.store)
            .i32()
            .ok_or(InstanceError::HeapBaseIsNotI32)?;

        Ok(heap_base as u32)
    }
}
