// This file is part of Gear.
//
// Copyright (C) 2021-2022 Gear Technologies Inc.
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

//! Environment of the wasm execution
use crate::metadata::{funcs, result::Result, StoreData};
use wasmtime::{
    AsContext, AsContextMut, Caller, Extern, Func, Linker, Memory, MemoryType, Store, Trap,
};

/// Apply environment to wasm instance
pub fn apply(store: &mut Store<StoreData>, linker: &mut Linker<StoreData>) -> Result<()> {
    let memory = Memory::new(store.as_context_mut(), MemoryType::new(256, None))?;

    // Define memory
    linker.define("env", "memory", Extern::Memory(memory))?;

    // Define functions
    linker.define("env", "alloc", funcs::alloc(store.as_context_mut(), memory))?;
    linker.define("env", "free", funcs::free(store.as_context_mut()))?;

    linker.define(
        "env",
        "gr_debug",
        funcs::gr_debug(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_panic",
        funcs::gr_panic(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_oom_panic",
        funcs::gr_oom_panic(store.as_context_mut()),
    )?;

    linker.define(
        "env",
        "gr_read",
        funcs::gr_read(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_reply",
        funcs::gr_reply(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_error",
        funcs::gr_error(store.as_context_mut(), memory),
    )?;

    linker.define(
        "env",
        "gr_size",
        funcs::gr_size(store.as_context_mut(), memory),
    )?;

    Ok(())
}
