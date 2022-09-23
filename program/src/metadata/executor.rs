// Copyright (C) 2021-2022 Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

//! WASM executor for getting metadata from `*.meta.wasm`
use crate::{
    api::types::GearPages,
    metadata::{
        env,
        ext::Ext,
        result::{Error, Result},
        StoreData,
    },
};
use wasmtime::{
    AsContext, AsContextMut, Config, Engine, Extern, Func, Instance, Linker, Memory, Module, Store,
    Val,
};

const PAGE_SIZE: usize = 4096;
const META_STATE: &str = "meta_state";

/// Exeucte wasm binary
pub fn execute<R>(wasm: &[u8], f: impl Fn(Reader) -> Result<R>) -> Result<R> {
    let engine = Engine::default();
    let module = Module::new(&engine, &mut &wasm[..])?;
    let mut store = Store::new(&engine, Default::default());

    // 1. Construct linker.
    let mut linker = <Linker<StoreData>>::new(&engine);
    env::apply(&mut store, &mut linker)?;

    // 2. Construct instance.
    let instance = linker.instantiate(&mut store, &module)?;

    f(Reader {
        instance,
        linker,
        store,
    })
}

/// Reader for reading metadata declaration from "*.meta.wasm"
pub struct Reader {
    instance: Instance,
    linker: Linker<StoreData>,
    pub store: Store<StoreData>,
}

impl Reader {
    /// Get function from wasm instance
    pub fn func(&mut self, name: impl AsRef<str>) -> Result<Func> {
        let meta = name.as_ref();
        self.instance
            .get_func(self.store.as_context_mut(), meta)
            .ok_or_else(|| Error::MetadataNotExists(meta.into()))
    }

    /// Get memory from wasm instance
    pub fn memory(&mut self) -> Result<Memory> {
        if let Some(Extern::Memory(mem)) =
            self.linker
                .get(self.store.as_context_mut(), "env", "memory")
        {
            Ok(mem)
        } else {
            Err(Error::MemoryNotExists)
        }
    }

    /// Read metadata from meta type
    pub fn meta(&mut self, memory: &Memory, meta: &str) -> Result<Vec<u8>> {
        let mut res = [Val::null()];
        self.func(meta)?.call(&mut self.store, &[], &mut res)?;

        let at = if let Val::I32(at) = res[0] {
            at as usize
        } else {
            return Err(Error::ReadMetadataFailed(meta.into()));
        };

        let mem = memory.data(&self.store);

        let mut ptr_bytes = [0; 4];
        ptr_bytes.copy_from_slice(&mem[at..(at + 4)]);
        let ptr = i32::from_le_bytes(ptr_bytes) as usize;

        let mut len_bytes = [0; 4];
        len_bytes.copy_from_slice(&mem[(at + 4)..(at + 8)]);
        let len = i32::from_le_bytes(len_bytes) as usize;

        Ok(mem[ptr..(ptr + len)].into())
    }

    /// Read the state of this program.
    pub fn state(
        &mut self,
        initial_size: u64,
        pages: GearPages,
        msg: Vec<u8>,
        timestamp: u64,
        height: u64,
    ) -> Result<Vec<u8>> {
        // 1. Grow memory if needed.
        let mem = self.memory()?;
        let mem_size = mem.size(&self.store);
        if mem_size < initial_size {
            mem.grow(self.store.as_context_mut(), initial_size - mem_size)?;
        }

        // 2. Update the host state in ext.
        let data = self.store.data_mut();
        data.msg = msg;
        data.timestamp = timestamp;
        data.height = height;

        // 3. Apply pages to the current wasm module
        let mem_mut = mem.data_mut(self.store.as_context_mut());
        for (idx, page) in pages {
            let start = (idx as usize) * PAGE_SIZE;
            let end = start + PAGE_SIZE;

            mem_mut[start..end].copy_from_slice(&page);
        }

        self.meta(&mem, META_STATE)
    }
}
