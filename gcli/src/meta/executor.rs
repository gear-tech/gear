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
pub struct HostState {
    pub msg: Vec<u8>,
    pub timestamp: u64,
    pub height: u64,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            msg: Vec::with_capacity(256),
            timestamp: 0,
            height: 0,
        }
    }
}

/// Executes the WASM code.
pub fn execute(wasm: &[u8], method: &str) -> Result<Vec<u8>> {
    assert!(gear_lazy_pages_interface::try_to_enable_lazy_pages(
        PAGE_STORAGE_PREFIX
    ));

    let engine = Engine::default();
    let module = Module::new(&engine, &wasm[..]).unwrap();

    let mut store = Store::new(&engine, HostState::default());
    let mut linker = <Linker<HostState>>::new();

    // Execution environment
    //
    // (import "env" "memory" (memory (;0;) 17))
    // (import "env" "gr_read" (func (;0;) (type 5)))
    // (import "env" "alloc" (func (;1;) (type 6)))
    // (import "env" "free" (func (;2;) (type 6)))
    // (import "env" "gr_size" (func (;3;) (type 4)))
    // (import "env" "gr_reply" (func (;4;) (type 5)))
    // (import "env" "gr_panic" (func (;5;) (type 0)))
    // (import "env" "gr_oom_panic" (func (;6;) (type 7)))
    {
        let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None)).unwrap();
        linker
            .define("env", "memory", Extern::Memory(memory))
            .unwrap();

        linker
            .define("env", "gr_read", funcs::gr_read(&mut store, memory.clone()))
            .unwrap();

        linker
            .define("env", "alloc", funcs::alloc(&mut store, memory.clone()))
            .unwrap();

        linker
            .define("env", "free", funcs::free(&mut store))
            .unwrap();

        linker
            .define("env", "gr_size", funcs::gr_size(&mut store, memory))
            .unwrap();

        linker
            .define("env", "gr_reply", funcs::gr_reply(&mut store))
            .unwrap();

        linker
            .define("env", "gr_panic", funcs::gr_panic(&mut store, memory))
            .unwrap();

        linker
            .define("env", "gr_oom_panic", funcs::gr_oom_panic(&mut store))
            .unwrap();
    }

    let instance = linker
        .instantiate(&mut store, &module)
        .unwrap()
        .start(&mut store)
        .unwrap();

    let metadata = instance
        .get_export(&store, method)
        .and_then(Extern::into_func)
        .ok_or_else(|| anyhow!("could not find function \"metadata\""))
        .unwrap()
        .typed::<(), (), _>(&mut store)
        .unwrap();

    metadata.call(&mut store, ()).unwrap();
    Ok(store.state().msg.clone())
}
