// This file is part of Gear.

// Copyright (C) 2021 Gear Technologies Inc.
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

//! Wasmi environment for running a module.

use wasmi::{
    Error as InterpreterError, Externals, FuncInstance, FuncRef, ImportsBuilder, MemoryDescriptor,
    MemoryRef, ModuleImportResolver, ModuleInstance, ModuleRef, RuntimeArgs, RuntimeValue,
    Signature, Trap, TrapKind, ValueType,
};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

use super::memory::MemoryWrap;

use gear_core::env::{Ext, LaterExt};
use gear_core::memory::{Memory, PageBuf, PageNumber};

use crate::funcs;

/// Struct for implementing host functions.
struct Runtime<E: Ext + 'static> {
    ext: LaterExt<E>,
}

#[derive(FromPrimitive)]
enum FuncIndex {
    Alloc = 1,
    Charge,
    Debug,
    Free,
    Gas,
    GasAvailable,
    MsgId,
    Read,
    Reply,
    ReplyPush,
    ReplyTo,
    Send,
    SendAndWait,
    SendCommit,
    SendInit,
    SendPush,
    Size,
    Source,
    Value,
    Wait,
    Wake,
}

macro_rules! func_instance {
    ($idx:ident, $($param:expr $(,)?)* => $res:expr) => {
        FuncInstance::alloc_host(
            Signature::new(&[$($param,)*][..], $res),
            FuncIndex::$idx as usize,
        )
    };
}

impl<E: Ext + 'static> Externals for Runtime<E> {
    /// Host functions implementations.
    fn invoke_index(
        &mut self,
        index: usize,
        args: RuntimeArgs,
    ) -> Result<Option<RuntimeValue>, Trap> {
        match FromPrimitive::from_usize(index) {
            Some(FuncIndex::Alloc) => funcs::alloc(self.ext.clone())(args.nth(0))
                .map(|v| Some(RuntimeValue::I32(v as i32)))
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Debug) => funcs::debug(self.ext.clone())(args.nth(0), args.nth(1))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Free) => funcs::free(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Gas) => funcs::gas(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::InvalidConversionToInt)),

            Some(FuncIndex::GasAvailable) => Ok(Some(RuntimeValue::I64(funcs::gas_available(
                self.ext.clone(),
            )()))),

            Some(FuncIndex::MsgId) => funcs::msg_id(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Read) => {
                funcs::read(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::Reply) => {
                funcs::reply(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2), args.nth(3))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::ReplyPush) => {
                funcs::reply_push(self.ext.clone())(args.nth(0), args.nth(1))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::ReplyTo) => funcs::reply_to(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Send) => funcs::send(self.ext.clone())(
                args.nth(0),
                args.nth(1),
                args.nth(2),
                args.nth(3),
                args.nth(4),
                args.nth(5),
            )
            .map(|_| None)
            .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::SendAndWait) => funcs::send(self.ext.clone())(
                args.nth(0),
                args.nth(1),
                args.nth(2),
                args.nth(3),
                args.nth(4),
            )
            .map(|_| None)
            .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::SendCommit) => funcs::send_commit(self.ext.clone())(
                args.nth(0),
                args.nth(1),
                args.nth(2),
                args.nth(3),
                args.nth(4),
            )
            .map(|_| None)
            .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::SendInit) => funcs::send_init(self.ext.clone())()
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::SendPush) => {
                funcs::send_push(self.ext.clone())(args.nth(0), args.nth(1), args.nth(2))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::UnexpectedSignature))
            }

            Some(FuncIndex::Size) => Ok(Some(RuntimeValue::I32(funcs::size(self.ext.clone())()))),

            Some(FuncIndex::Source) => funcs::source(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Value) => funcs::value(self.ext.clone())(args.nth(0))
                .map(|_| None)
                .map_err(|_| Trap::new(TrapKind::UnexpectedSignature)),

            Some(FuncIndex::Wait) => {
                funcs::wait(self.ext.clone())()
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::Unreachable)) // TODO: Define custom HostError for "exit" trap
            }

            Some(FuncIndex::Wake) => {
                funcs::wake(self.ext.clone())(args.nth(0))
                    .map(|_| None)
                    .map_err(|_| Trap::new(TrapKind::Unreachable)) // TODO: Define custom HostError for "exit" trap
            }

            _ => panic!("unknown function index"),
        }
    }
}

impl<E: Ext + 'static> ModuleImportResolver for Environment<E> {
    /// Provide imports corresponding concrete reference.
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &Signature,
    ) -> Result<FuncRef, InterpreterError> {
        let func_ref = match field_name {
            "alloc" => func_instance!(Alloc, ValueType::I32 => Some(ValueType::I32)),
            "free" => func_instance!(Free, ValueType::I32 => None),
            "gas" => func_instance!(Gas, ValueType::I32 => None),
            "gr_gas_available" => func_instance!(GasAvailable, => Some(ValueType::I64)),
            "gr_debug" => func_instance!(Debug, ValueType::I32, ValueType::I32 => None),
            "gr_msg_id" => func_instance!(MsgId, ValueType::I32 => None),
            "gr_read" => {
                func_instance!(Read, ValueType::I32, ValueType::I32, ValueType::I32 => None)
            }
            "gr_reply" => func_instance!(Reply, ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32 => None),
            "gr_reply_push" => {
                func_instance!(ReplyPush, ValueType::I32, ValueType::I32 => None)
            }
            "gr_reply_to" => func_instance!(ReplyTo, ValueType::I32 => None),
            "gr_send" => func_instance!(Send, ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32,
                ValueType::I32 => None),
            "gr_send_and_wait" => func_instance!(SendAndWait, ValueType::I32,
                    ValueType::I32,
                    ValueType::I32,
                    ValueType::I32,
                    ValueType::I32 => None),
            "gr_send_commit" => func_instance!(SendCommit, ValueType::I32, ValueType::I32 => None),
            "gr_send_init" => func_instance!(SendInit, ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I64,
                ValueType::I32 => Some(ValueType::I32)),
            "gr_send_push" => {
                func_instance!(SendPush, ValueType::I32, ValueType::I32, ValueType::I32 => None)
            }
            "gr_size" => func_instance!(Size, => Some(ValueType::I32)),
            "gr_source" => func_instance!(Source, ValueType::I32 => None),
            "gr_value" => func_instance!(Value, ValueType::I32 => None),
            "gr_wait" => func_instance!(Wait, => None),
            "gr_wake" => func_instance!(Wake, ValueType::I32 => None),

            _ => {
                return Err(InterpreterError::Function(format!(
                    "host module doesn't export function with name {}",
                    field_name
                )));
            }
        };
        Ok(func_ref)
    }

    /// Map module memory to host memory
    fn resolve_memory(
        &self,
        _field_name: &str,
        _memory_type: &MemoryDescriptor,
    ) -> Result<MemoryRef, InterpreterError> {
        let mem = match self.memory.as_ref() {
            Some(memory) => memory,
            None => panic!("Memory is None"),
        };

        let mem = match mem.as_any().downcast_ref::<MemoryRef>() {
            Some(memory) => memory,
            None => panic!("Memory is not wasmi::MemoryRef"),
        };

        Ok(mem.clone())
    }
}

/// Environment to run one module at a time providing Ext.
pub struct Environment<E: Ext + 'static> {
    ext: LaterExt<E>,
    memory: Option<Box<dyn Memory>>,
}

impl<E: Ext + 'static> Default for Environment<E> {
    fn default() -> Self {
        Environment::<E>::new()
    }
}

impl<E: Ext + 'static> Environment<E> {
    /// New environment.
    ///
    /// To run actual function with provided external environment, `setup_and_run` should be used.
    pub fn new() -> Self {
        Self {
            ext: LaterExt::new(),
            memory: None,
        }
    }

    /// Setup external environment and run closure.
    ///
    /// Setup external environment by providing `ext`, run nenwly initialized instance created from
    /// provided `module`, do anything inside a `func` delegate.
    ///
    /// This will also set the beginning of the memory region to the `static_area` content _after_
    /// creatig instance.
    pub fn setup_and_run(
        &mut self,
        ext: E,
        binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        entry_point: &str,
    ) -> (anyhow::Result<()>, E) {
        self.memory = Some(memory.clone());

        let module = wasmi::Module::from_buffer(binary).expect("Error creating module");

        let mut imports = ImportsBuilder::new();
        imports.push_resolver("env", self);

        let instance = ModuleInstance::new(&module, &imports)
            .expect("failed to instantiate wasm module")
            .assert_no_start();

        self.ext.set(ext);
        let mut runtime = Runtime {
            ext: self.ext.clone(),
        };

        let result = self.run_inner(instance, memory_pages, memory, move |instance| {
            let result = instance.invoke_export(entry_point, &[], &mut runtime);
            if let Err(InterpreterError::Trap(trap)) = &result {
                // TODO: Define custom HostError for `gr_wait` trap
                if let TrapKind::Unreachable = trap.kind() {
                    // We don't propagate a trap from `gr_wait`
                    return Ok(());
                }
            }
            result
                .map_err(|err| anyhow::format_err!("Failed export: {:?}", err))
                .map(|_| ())
        });

        let ext = self.ext.unset();

        (result, ext)
    }

    /// Create memory inside this environment.
    pub fn create_memory(&self, total_pages: u32) -> MemoryWrap {
        MemoryWrap::new(
            wasmi::MemoryInstance::alloc(wasmi::memory_units::Pages(total_pages as usize), None)
                .expect("Create env memory fail"),
        )
    }

    fn run_inner(
        &mut self,
        module: ModuleRef,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        memory: &dyn Memory,
        func: impl FnOnce(ModuleRef) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        // Set module memory.
        memory
            .set_pages(memory_pages)
            .map_err(|_| anyhow::anyhow!("Can't set module memory"))?;

        func(module)
    }
}
