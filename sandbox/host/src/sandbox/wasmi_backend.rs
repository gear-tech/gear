// This file is part of Gear.

// Copyright (C) Gear Technologies Inc.
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

//! Wasmi specific impls for sandbox

use std::{fmt, rc::Rc, slice};

use codec::{Decode, Encode};
use gear_sandbox_env::{HostError, Instantiate};
use region::{Allocation, Protection};
use sandbox_wasmi::{
    core::{Pages, Trap, UntypedVal},
    Config, Engine, ExternType, Linker, MemoryType, Module, StackLimits, StoreContext,
    StoreContextMut,
};

use sp_wasm_interface_common::{util, Pointer, ReturnValue, Value, WordSize};

use crate::{
    error::{self, Error},
    sandbox::{
        BackendInstanceBundle, GuestEnvironment, GuestExternals, GuestFuncIndex, Imports,
        InstantiationError, Memory, SandboxInstance, SupervisorContext,
    },
    store_refcell,
    util::MemoryTransfer,
};

type Store = sandbox_wasmi::Store<()>;
pub type StoreRefCell = store_refcell::StoreRefCell<Store>;

environmental::environmental!(SupervisorContextStore: trait SupervisorContext);

#[derive(Debug)]
struct CustomHostError(String);

impl fmt::Display for CustomHostError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "HostError: {}", self.0)
    }
}

impl sandbox_wasmi::core::HostError for CustomHostError {}

/// Construct trap error from specified message
//fn trap(msg: &'static str) -> Trap {
//    Trap::host(CustomHostError(msg.into()))
//}

fn into_wasmi_val(value: Value) -> sandbox_wasmi::Val {
    match value {
        Value::I32(val) => sandbox_wasmi::Val::I32(val),
        Value::I64(val) => sandbox_wasmi::Val::I64(val),
        Value::F32(val) => sandbox_wasmi::Val::F32(val.into()),
        Value::F64(val) => sandbox_wasmi::Val::F64(val.into()),
    }
}

fn into_wasmi_result(value: ReturnValue) -> Vec<sandbox_wasmi::Val> {
    match value {
        ReturnValue::Value(v) => vec![into_wasmi_val(v)],
        ReturnValue::Unit => vec![],
    }
}

fn into_value(value: sandbox_wasmi::Val) -> Option<Value> {
    match value {
        sandbox_wasmi::Val::I32(val) => Some(Value::I32(val)),
        sandbox_wasmi::Val::I64(val) => Some(Value::I64(val)),
        sandbox_wasmi::Val::F32(val) => Some(Value::F32(val.into())),
        sandbox_wasmi::Val::F64(val) => Some(Value::F64(val.into())),
        _ => None,
    }
}

/// Wasmi specific context
pub struct Backend {
    store: Rc<StoreRefCell>,
}

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend {
    pub fn new() -> Self {
        let register_len = size_of::<UntypedVal>();

        const DEFAULT_MIN_VALUE_STACK_HEIGHT: usize = 1024;
        const DEFAULT_MAX_VALUE_STACK_HEIGHT: usize = 1024 * DEFAULT_MIN_VALUE_STACK_HEIGHT;
        const DEFAULT_MAX_RECURSION_DEPTH: usize = 16384;

        let mut config = Config::default();
        config.set_stack_limits(
            StackLimits::new(
                DEFAULT_MIN_VALUE_STACK_HEIGHT / register_len,
                DEFAULT_MAX_VALUE_STACK_HEIGHT / register_len,
                DEFAULT_MAX_RECURSION_DEPTH,
            )
            .expect("infallible"),
        );

        let engine = Engine::new(&config);
        let store = Store::new(&engine, ());
        Backend {
            store: Rc::new(StoreRefCell::new(Store::new(&engine, ()))),
        }
    }

    pub fn store(&self) -> &Rc<StoreRefCell> {
        &self.store
    }
}

/// Allocate new memory region
pub fn new_memory(
    store: Rc<StoreRefCell>,
    initial: u32,
    maximum: Option<u32>,
) -> crate::error::Result<Memory> {
    let ty =
        MemoryType::new(initial, maximum).map_err(|error| Error::Sandbox(error.to_string()))?;
    let mut alloc = region::alloc(u32::MAX as usize, Protection::READ_WRITE)
        .unwrap_or_else(|err| unreachable!("Failed to allocate memory: {err}"));
    // # Safety:
    //
    // `wasmi::Memory::new_static()` requires static lifetime so we convert our buffer to it
    // but actual lifetime of the buffer is lifetime of `Store<T>` itself,
    // so memory will be deallocated when `Store<T>` is dropped.
    //
    // Also, according to Rust drop order semantics, `wasmi::Store<T>` will be dropped first and
    // only then our allocated memories will be freed to ensure they are not used anymore.
    let raw = unsafe { slice::from_raw_parts_mut::<'static, u8>(alloc.as_mut_ptr(), alloc.len()) };
    let memory = sandbox_wasmi::Memory::new_static(&mut *store.borrow_mut(), ty, raw)
        .map_err(|error| Error::Sandbox(error.to_string()))?;

    Ok(Memory::Wasmi(MemoryWrapper::new(memory, store, alloc)))
}

/// Wasmi provides direct access to its memory using slices.
///
/// This wrapper limits the scope where the slice can be taken to
#[derive(Debug, Clone)]
pub struct MemoryWrapper {
    memory: sandbox_wasmi::Memory,
    store: Rc<StoreRefCell>,
    //alloc: Allocation,
}

impl MemoryWrapper {
    /// Take ownership of the memory region and return a wrapper object
    fn new(memory: sandbox_wasmi::Memory, store: Rc<StoreRefCell>, alloc: Allocation) -> Self {
        Self {
            memory,
            store,
            //alloc,
        }
    }
}

impl MemoryTransfer for MemoryWrapper {
    fn read(&self, source_addr: Pointer<u8>, size: usize) -> error::Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(size);
        let ctx = self.store.borrow();
        self.memory
            .read(&*ctx, source_addr.into(), &mut buffer)
            .map_err(|_| error::Error::Other("memory read is out of bounds".into()));

        Ok(buffer)
    }

    fn read_into(&self, source_addr: Pointer<u8>, destination: &mut [u8]) -> error::Result<()> {
        let ctx = self.store.borrow();
        self.memory
            .read(&*ctx, source_addr.into(), destination)
            .map_err(|_| error::Error::Other("memory read is out of bounds".into()));

        Ok(())
    }

    fn write_from(&self, dest_addr: Pointer<u8>, source: &[u8]) -> error::Result<()> {
        let mut ctx = self.store.borrow_mut();
        self.memory
            .write(&mut *ctx, dest_addr.into(), source)
            .map_err(|_| error::Error::Other("memory write is out of bounds".into()));

        Ok(())
    }

    fn memory_grow(&mut self, pages: u32) -> error::Result<u32> {
        let mut ctx = self.store.borrow_mut();
        self.memory.grow(&mut *ctx, pages).map_err(|e| {
            Error::Sandbox(format!("Cannot grow memory in wasmi sandbox executor: {e}",))
        })
    }

    fn memory_size(&mut self) -> u32 {
        let ctx = self.store.borrow();
        self.memory.size(&*ctx)
    }

    fn get_buff(&mut self) -> *mut u8 {
        let ctx = self.store.borrow_mut();
        self.memory.data_ptr(&*ctx)
    }
}

/// Get global value by name
pub fn get_global(instance: &sandbox_wasmi::Instance, store: &Store, name: &str) -> Option<Value> {
    into_value(instance.get_global(store, name)?.get(store))
}

/// Set global value by name
pub fn set_global(
    instance: &sandbox_wasmi::Instance,
    store: &mut Store,
    name: &str,
    value: Value,
) -> std::result::Result<Option<()>, error::Error> {
    let global = match instance.get_global(&*store, name) {
        Some(e) => e,
        None => return Ok(None),
    };

    global
        .set(store, into_wasmi_val(value))
        .map(Some)
        .map_err(|e| Error::Sandbox(e.to_string()))
}

/// Instantiate a module within a sandbox context
pub fn instantiate(
    version: Instantiate,
    context: &Backend,
    wasm: &[u8],
    guest_env: GuestEnvironment,
    supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<SandboxInstance, InstantiationError> {
    todo!()
}

/// Invoke a function within a sandboxed module
pub fn invoke(
    instance: &sandbox_wasmi::Instance,
    store: &Rc<StoreRefCell>,
    export_name: &str,
    args: &[Value],
    supervisor_context: &mut dyn SupervisorContext,
) -> std::result::Result<Option<Value>, Error> {
    todo!()
}
