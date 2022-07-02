// This file is part of Gear.

// Copyright (C) 2021-2022 Gear Technologies Inc.
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

use core_processor::{Ext, ProcessorExt};
use gear_backend_common::TerminationReason;
use gear_backend_wasmtime::{env::StoreData, funcs_tree};
use gear_core::{
    env::{Ext as ExtTrait, ExtCarrier},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    memory::{AllocationsContext, PageBuf, PageNumber, WasmPageNumber},
    message::{IncomingMessage, MessageContext, Payload},
    program::Program,
};
use std::{collections::BTreeMap, mem};
use wasmtime::{
    Config, Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store,
    Val,
};

use crate::{TestError, TestResult, MAILBOX_THRESHOLD};

/// Binary meta-functions executor for testing purposes
pub(crate) struct WasmExecutor {
    instance: Instance,
    store: Store<StoreData<Ext>>,
    memory: WasmtimeMemory,
}

impl WasmExecutor {
    /// Creates a WasmExecutor instance from a program.
    /// Also uses provided memory pages for future execution
    pub(crate) fn new(
        program: &Program,
        meta_binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        payload: Option<Payload>,
    ) -> TestResult<Self> {
        let ext = WasmExecutor::build_ext(program, payload.unwrap_or_default());
        let ext_carrier = ExtCarrier::new(ext);
        let store_data = StoreData {
            ext: ext_carrier.cloned(),
            termination_reason: TerminationReason::Success,
        };

        let config = Config::new();
        let engine = Engine::new(&config)?;
        let mut store = Store::<StoreData<Ext>>::new(&engine, store_data);

        let module = Module::new(&engine, meta_binary)?;

        let mut memory =
            WasmtimeMemory::new(&mut store, MemoryType::new(program.static_pages().0, None))?;

        let funcs = funcs_tree::build(&mut store, memory, None);
        let mut externs = Vec::with_capacity(module.imports().len());
        for import in module.imports() {
            if import.module() != "env" {
                return Err(TestError::InvalidImportModule(import.module().to_string()));
            }
            match import.name() {
                Some("memory") => externs.push(Extern::Memory(memory)),
                Some(key) => {
                    if funcs.contains_key(key) {
                        externs.push(funcs[key].into())
                    } else {
                        return Err(TestError::UnsupportedFunction(key.to_string()));
                    }
                }
                _ => continue,
            };
        }

        let instance = Instance::new(&mut store, &module, &externs)?;
        WasmExecutor::set_pages(&mut store, &mut memory, memory_pages)?;

        Ok(Self {
            instance,
            store,
            memory,
        })
    }

    /// Executes non-void function by provided name.
    /// Panics if function is void
    pub(crate) fn execute(&mut self, function_name: &str) -> TestResult<Vec<u8>> {
        let function = self.get_function(function_name)?;
        let mut ptr_to_result_array = [Val::I32(0)];

        function
            .call(&mut self.store, &[], &mut ptr_to_result_array)
            .map_err(|err| {
                if let Some(processor_error) = self
                    .store
                    .data()
                    .ext
                    .with(|a| a.error_explanation.clone())
                    .expect("`with` is expected to be called only after `inner` is set")
                {
                    processor_error.into()
                } else {
                    TestError::WasmtimeError(err)
                }
            })?;

        match ptr_to_result_array[0] {
            Val::I32(ptr_to_result) => self.read_result(ptr_to_result),
            _ => Err(TestError::InvalidReturnType),
        }
    }

    fn build_ext(program: &Program, payload: Payload) -> Ext {
        Ext::new(
            GasCounter::new(u64::MAX),
            GasAllowanceCounter::new(u64::MAX),
            ValueCounter::new(u128::MAX),
            AllocationsContext::new(
                program.get_allocations().clone(),
                program.static_pages(),
                WasmPageNumber(512u32),
            ),
            MessageContext::new(
                IncomingMessage::new(Default::default(), Default::default(), payload, 0, 0, None),
                program.id(),
                None,
            ),
            Default::default(),
            Default::default(),
            0,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            MAILBOX_THRESHOLD,
        )
    }

    fn get_function(&mut self, function_name: &str) -> TestResult<Func> {
        self.instance
            .get_func(&mut self.store, function_name)
            .ok_or_else(|| TestError::FunctionNotFound(function_name.to_string()))
    }

    fn read_result(&mut self, ptr_to_result_data: i32) -> TestResult<Vec<u8>> {
        let offset = ptr_to_result_data as usize;

        // Reading a fat pointer from the `offset`
        let mut ptr = [0_u8; mem::size_of::<i32>()];
        let mut len = [0_u8; mem::size_of::<i32>()];

        self.memory.read(&self.store, offset, &mut ptr)?;

        self.memory
            .read(&self.store, offset + ptr.len(), &mut len)?;

        let ptr = i32::from_ne_bytes(ptr) as usize;
        let len = i32::from_ne_bytes(len) as usize;

        // Reading a vector from `ptr`
        let mut result = vec![0; len];

        self.memory.read(&self.store, ptr, &mut result)?;

        Ok(result)
    }

    fn set_pages<T: ExtTrait>(
        mut store: &mut Store<StoreData<T>>,
        memory: &mut WasmtimeMemory,
        pages: &BTreeMap<PageNumber, Box<PageBuf>>,
    ) -> TestResult<()> {
        let memory_size = WasmPageNumber(memory.size(&mut store) as u32);
        for (page_number, buffer) in pages {
            let wasm_page_number = page_number.to_wasm_page();
            if memory_size <= wasm_page_number {
                return Err(TestError::InsufficientMemory(memory_size, wasm_page_number));
            }
            memory.write(&mut store, page_number.offset(), &buffer[..])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod meta_tests {
    use crate::{Program, System};
    use codec::{Decode, Encode};
    use gear_core::ids::ProgramId;

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
    pub struct Id {
        pub decimal: u64,
        pub hex: Vec<u8>,
    }

    #[derive(Encode, Clone, Decode)]
    pub struct Person {
        pub surname: String,
        pub name: String,
    }

    #[derive(Encode, Clone, Decode)]
    pub struct Wallet {
        pub id: Id,
        pub person: Person,
    }

    #[derive(Encode, Decode)]
    pub struct MessageInitIn {
        pub amount: u8,
        pub currency: String,
    }

    #[test]
    fn test_happy_case() {
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let result: Vec<Wallet> = program
            .meta_state(&Some(Id {
                decimal: 2,
                hex: vec![2u8],
            }))
            .expect("Meta_state failed");

        assert_eq!(result.encode(), vec![0]);
    }

    #[test]
    fn test_meta_extension_happy_case() {
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.meta.wasm",
        );

        let result: Vec<Wallet> = program
            .meta_state(&Some(Id {
                decimal: 2,
                hex: vec![2u8],
            }))
            .expect("Meta_state failed");

        assert_eq!(result.encode(), vec![0]);
    }

    #[test]
    fn test_manager_executions_coworking() {
        let user_id: ProgramId = 100.into();
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let expected_result = vec![Wallet {
            id: Id {
                decimal: 2,
                hex: vec![2u8],
            },
            person: Person {
                surname: "OtherName".into(),
                name: "OtherSurname".into(),
            },
        }];

        let expected_id = Some(expected_result.first().unwrap().id.clone());

        let run_result = program.send(
            user_id,
            MessageInitIn {
                amount: 1,
                currency: "1".to_string(),
            },
        );
        assert!(!run_result.main_failed);

        let result: Vec<Wallet> = program.meta_state(&expected_id).expect("Meta_state failed");

        assert_eq!(result.encode(), expected_result.encode());
    }

    #[test]
    #[should_panic(expected = "Meta_state failed: \"No function with name: \\\"fsd314f\\\"\"")]
    fn test_failing_with_unknown_function() {
        let unknown_function_name = "fsd314f";
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        system
            .0
            .borrow_mut()
            .call_meta(&program.id, None, unknown_function_name)
            .expect("Meta_state failed");
    }

    #[test]
    #[should_panic(expected = "Meta_state failed: \"expected 0 results, got 1\"")]
    fn test_failing_with_void_function() {
        let void_function_name = "init";
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        system
            .0
            .borrow_mut()
            .call_meta(&program.id, None, void_function_name)
            .expect("Meta_state failed");
    }

    #[test]
    #[should_panic(
        expected = "Meta_state failed: \"'failed to decode input argument: Decode(Error)', meta/src/lib.rs:157:42\""
    )]
    fn test_failing_with_empty_payload() {
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        program
            .meta_state_empty::<Vec<Wallet>>()
            .expect("Meta_state failed");
    }
}
