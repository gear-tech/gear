// This file is part of Gear.
//
// Copyright (C) 2021-2023 Gear Technologies Inc.
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

use super::funcs;
use anyhow::{anyhow, Result};
use wasmi::{AsContextMut, Engine, Extern, Linker, Memory, MemoryType, Module, Store};

const PAGE_STORAGE_PREFIX: [u8; 32] = *b"gcligcligcligcligcligcligcligcli";

/// HostState for the WASM executor
#[derive(Default)]
pub struct HostState {
    pub msg: Vec<u8>,
    pub timestamp: u64,
    pub height: u64,
}

/// Executes the WASM code.
pub fn execute(wasm: &[u8], method: &str) -> Result<Vec<u8>> {
    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        PAGE_STORAGE_PREFIX
    ));

    let engine = Engine::default();
    let module = Module::new(&engine, wasm).unwrap();

    let mut store = Store::new(&engine, HostState::default());
    let mut linker = <Linker<HostState>>::new();

    // Execution environment
    {
        let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None)).unwrap();
        linker.define("env", "memory", Extern::Memory(memory))?;
        linker.define("env", "gr_read", funcs::gr_read(&mut store, memory))?;
        linker.define("env", "alloc", funcs::alloc(&mut store, memory))?;
        linker.define("env", "free", funcs::free(&mut store))?;
        linker.define("env", "gr_size", funcs::gr_size(&mut store, memory))?;
        linker.define("env", "gr_reply", funcs::gr_reply(&mut store, memory))?;
        linker.define("env", "gr_panic", funcs::gr_panic(&mut store, memory))?;
        linker.define("env", "gr_oom_panic", funcs::gr_oom_panic(&mut store))?;
        linker.define("env", "gr_out_of_gas", funcs::gr_out_of_gas(&mut store))?;
        linker.define("env", "gr_block_height", funcs::gr_block_height(&mut store))?;
        linker.define(
            "env",
            "gr_block_timestamp",
            funcs::gr_block_timestamp(&mut store),
        )?;
    }

    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)?;

    let metadata = instance
        .get_export(&store, method)
        .and_then(Extern::into_func)
        .ok_or_else(|| anyhow!("could not find function \"metadata\""))?
        .typed::<(), (), _>(&mut store)?;

    metadata.call(&mut store, ())?;
    Ok(store.state().msg.clone())
}
