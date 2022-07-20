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

use core_processor::{Ext, ProcessorContext, ProcessorExt};
use gear_backend_common::TerminationReason;
use gear_backend_wasmi::{
    env::{DefinedHostFunctions, EnvironmentDefinitionBuilder, Runtime, GuestExternals},
    funcs::{FuncError, FuncsHandler as Funcs},
    MemoryWrap,
};
use gear_core::{
    env::{Ext as ExtTrait, ExtCarrier},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    memory::{AllocationsContext, PageBuf, PageNumber, WasmPageNumber},
    message::{IncomingMessage, MessageContext, Payload},
    program::Program,
};
use std::{collections::BTreeMap, mem, ops::Deref};
use wasmi::{
    memory_units::Pages, Externals, FuncInstance, FuncRef, GlobalDescriptor, GlobalRef, HostError,
    ImportResolver, MemoryDescriptor, MemoryInstance, MemoryRef, ModuleInstance, ModuleRef,
    NopExternals, RuntimeArgs, RuntimeValue,
};

use crate::{Result, TestError, MAILBOX_THRESHOLD};

/// Binary meta-functions executor for testing purposes
pub(crate) struct WasmExecutor {
    instance: ModuleRef,
    store: Runtime<Ext>,
    memory: MemoryRef,
    defined_host_functions: DefinedHostFunctions<Runtime<Ext>, <Ext as ExtTrait>::Error>,
}

impl WasmExecutor {
    /// Creates a WasmExecutor instance from a program.
    /// Also uses provided memory pages for future execution
    pub(crate) fn new(
        program: &Program,
        meta_binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        payload: Option<Payload>,
    ) -> Result<Self> {
        let ext = WasmExecutor::build_ext(program, payload.unwrap_or_default());
        let mut builder: EnvironmentDefinitionBuilder<Runtime<Ext>, <Ext as ExtTrait>::Error> =
            EnvironmentDefinitionBuilder::new(
                ext.forbidden_funcs()
                    .clone()
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
            );

        builder.add_host_func("env", "forbidden", Funcs::forbidden);
        builder.add_host_func("env", "gr_block_height", Funcs::block_height);
        builder.add_host_func("env", "gr_block_timestamp", Funcs::block_timestamp);
        builder.add_host_func("env", "gr_create_program", Funcs::create_program);
        builder.add_host_func("env", "gr_create_program_wgas", Funcs::create_program_wgas);
        builder.add_host_func("env", "gr_debug", Funcs::debug);
        builder.add_host_func("env", "gr_error", Funcs::error);
        builder.add_host_func("env", "gr_exit", Funcs::exit);
        builder.add_host_func("env", "gr_exit_code", Funcs::exit_code);
        builder.add_host_func("env", "gr_gas_available", Funcs::gas_available);
        builder.add_host_func("env", "gr_leave", Funcs::leave);
        builder.add_host_func("env", "gr_msg_id", Funcs::msg_id);
        builder.add_host_func("env", "gr_origin", Funcs::origin);
        builder.add_host_func("env", "gr_program_id", Funcs::program_id);
        builder.add_host_func("env", "gr_read", Funcs::read);
        builder.add_host_func("env", "gr_reply", Funcs::reply);
        builder.add_host_func("env", "gr_reply_commit", Funcs::reply_commit);
        builder.add_host_func("env", "gr_reply_commit_wgas", Funcs::reply_commit_wgas);
        builder.add_host_func("env", "gr_reply_push", Funcs::reply_push);
        builder.add_host_func("env", "gr_reply_to", Funcs::reply_to);
        builder.add_host_func("env", "gr_reply_wgas", Funcs::reply_wgas);
        builder.add_host_func("env", "gr_send", Funcs::send);
        builder.add_host_func("env", "gr_send_commit", Funcs::send_commit);
        builder.add_host_func("env", "gr_send_commit_wgas", Funcs::send_commit_wgas);
        builder.add_host_func("env", "gr_send_init", Funcs::send_init);
        builder.add_host_func("env", "gr_send_push", Funcs::send_push);
        builder.add_host_func("env", "gr_send_wgas", Funcs::send_wgas);
        builder.add_host_func("env", "gr_size", Funcs::size);
        builder.add_host_func("env", "gr_source", Funcs::source);
        builder.add_host_func("env", "gr_value", Funcs::value);
        builder.add_host_func("env", "gr_value_available", Funcs::value_available);
        builder.add_host_func("env", "gr_wait", Funcs::wait);
        builder.add_host_func("env", "gr_wake", Funcs::wake);

        let ext_carrier = ExtCarrier::new(ext);

        let mem: MemoryRef = MemoryInstance::alloc(Pages(program.static_pages().0 as usize), None)?;

        builder.add_memory("env", "memory", mem.clone());
        builder.add_host_func("env", "alloc", Funcs::alloc);
        builder.add_host_func("env", "free", Funcs::free);
        builder.add_host_func("env", "gas", Funcs::gas);

        let runtime = Runtime {
            ext: ext_carrier,
            err: FuncError::Terminated(TerminationReason::Success),
            memory: MemoryWrap::new(mem.clone()),
        };

        let defined_host_functions = builder.defined_host_functions.clone();
        let instance = match ModuleInstance::new(
            &wasmi::Module::from_buffer(meta_binary).expect("wasmi can't load module binary"),
            &builder,
        ) {
            Ok(inst) => inst.not_started_instance().clone(),
            Err(e) => return Err(TestError::WasmiError(e.into())),
        };
        WasmExecutor::set_pages(mem.clone(), memory_pages)?;

        Ok(Self {
            instance,
            store: runtime,
            memory: mem,
            defined_host_functions,
        })
    }

    /// Executes non-void function by provided name.
    /// Panics if function is void
    pub(crate) fn execute(&mut self, function_name: &str) -> Result<Vec<u8>> {
        let mut externals = GuestExternals {
            state: &mut self.store,
            defined_host_functions: &self.defined_host_functions,
        };
        let res = self
            .instance
            .invoke_export(function_name, &[], &mut externals)
            .map_err(|err| {
                if let wasmi::Error::Function(_) = err {
                    return TestError::FunctionNotFound(function_name.to_string());
                }
                if let Some(processor_error) = self
                    .store
                    .ext
                    .with(|a| a.error_explanation.clone())
                    .expect("`with` is expected to be called only after `inner` is set")
                {
                    processor_error.into()
                } else {
                    TestError::WasmiError(err.into())
                }
            })?;

        match res {
            Some(ptr_to_result) => match ptr_to_result {
                RuntimeValue::I32(ptr_to_result) => self.read_result(ptr_to_result),
                _ => Err(TestError::InvalidReturnType),
            },
            _ => Err(TestError::InvalidReturnType),
        }
    }

    fn build_ext(program: &Program, payload: Payload) -> Ext {
        Ext::new(ProcessorContext {
            gas_counter: GasCounter::new(u64::MAX),
            gas_allowance_counter: GasAllowanceCounter::new(u64::MAX),
            value_counter: ValueCounter::new(u128::MAX),
            allocations_context: AllocationsContext::new(
                program.get_allocations().clone(),
                program.static_pages(),
                WasmPageNumber(512u32),
            ),
            message_context: MessageContext::new(
                IncomingMessage::new(Default::default(), Default::default(), payload, 0, 0, None),
                program.id(),
                None,
            ),
            block_info: Default::default(),
            config: Default::default(),
            existential_deposit: 0,
            origin: Default::default(),
            program_id: Default::default(),
            program_candidates_data: Default::default(),
            host_fn_weights: Default::default(),
            forbidden_funcs: Default::default(),
            mailbox_threshold: MAILBOX_THRESHOLD,
        })
    }

    fn read_result(&mut self, ptr_to_result_data: i32) -> Result<Vec<u8>> {
        let offset = ptr_to_result_data as usize;

        // Reading a fat pointer from the `offset`
        let mut ptr = [0_u8; mem::size_of::<i32>()];
        let mut len = [0_u8; mem::size_of::<i32>()];

        self.memory.get_into(offset as u32, &mut ptr)?;

        self.memory
            .get_into((offset + ptr.len()) as u32, &mut len)?;

        let ptr = i32::from_ne_bytes(ptr) as usize;
        let len = i32::from_ne_bytes(len) as usize;

        // Reading a vector from `ptr`
        let mut result = vec![0; len];

        self.memory.get_into(ptr as u32, &mut result)?;

        Ok(result)
    }

    fn set_pages(memory: MemoryRef, pages: &BTreeMap<PageNumber, Box<PageBuf>>) -> Result<()> {
        let memory_size = WasmPageNumber(memory.current_size().0 as u32);
        for (page_number, buffer) in pages {
            let wasm_page_number = page_number.to_wasm_page();
            if memory_size <= wasm_page_number {
                return Err(TestError::InsufficientMemory(memory_size, wasm_page_number));
            }
            memory.set(page_number.offset() as u32, &buffer[..])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod meta_tests {
    use crate::{Program, System, TestError};
    use codec::{Decode, Encode};
    use core_processor::ProcessorError;
    use gear_core::ids::ProgramId;

    #[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
    pub struct Id {
        pub decimal: u64,
        pub hex: Vec<u8>,
    }

    #[derive(Debug, Encode, Clone, Decode, PartialEq, Eq)]
    pub struct Person {
        pub surname: String,
        pub name: String,
    }

    #[derive(Debug, Encode, Clone, Decode, PartialEq, Eq)]
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
    fn happy_case() {
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

        assert_eq!(result, vec![]);
    }

    #[test]
    fn meta_extension_happy_case() {
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

        assert_eq!(result, vec![]);
    }

    #[test]
    fn manager_executions_coworking() {
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

        assert_eq!(result, expected_result);
    }

    #[test]
    fn failing_with_unknown_function() {
        let unknown_function_name = "fsd314f";
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let result = system
            .0
            .borrow_mut()
            .call_meta(&program.id, None, unknown_function_name);
        if let Err(ref err) = result {
            println!("{:?}", err);
        }
        assert!(
            matches!(result, Err(TestError::FunctionNotFound(func)) if func == unknown_function_name)
        );
    }

    #[test]
    fn failing_with_void_function() {
        let void_function_name = "init";
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let result = system
            .0
            .borrow_mut()
            .call_meta(&program.id, None, void_function_name);
        assert!(matches!(result, Err(TestError::FunctionNotFound(_))));
    }

    #[test]
    fn failing_with_empty_payload() {
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let result = program.meta_state_empty::<Vec<Wallet>>();
        assert!(matches!(
            result,
            Err(TestError::ExecutionError(ProcessorError::Panic(_)))
        ));
    }

    #[test]
    fn failing_without_meta_binary() {
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.opt.wasm",
        );

        let result = program.meta_state_empty::<Vec<Wallet>>();
        assert!(matches!(result, Err(TestError::MetaBinaryNotProvided)));
    }
}
