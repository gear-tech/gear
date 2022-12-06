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
    funcs::FuncError,
    funcs_tree,
    state::{HostState, State},
    wasmi::{
        core::Value, Engine, Error as WasmiError, Extern, Linker, Memory as WasmiMemory,
        MemoryType, Module, Store,
    },
    MemoryWrap,
};
use gear_core::{
    env::Ext as ExtTrait,
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    memory::{AllocationsContext, Memory, PageBuf, PageNumber, WasmPageNumber},
    message::{ContextSettings, IncomingMessage, MessageContext, Payload},
    program::Program,
    reservation::GasReserver,
};
use gear_wasm_instrument::{GLOBAL_NAME_ALLOWANCE, GLOBAL_NAME_GAS};
use std::{collections::BTreeMap, mem};

use crate::{
    manager::ExtManager, Result, TestError, MAILBOX_THRESHOLD, MAX_RESERVATIONS, RESERVATION_COST,
    RESERVE_FOR, WAITLIST_COST, WRITE_COST,
};

/// Binary meta-functions executor for testing purposes
pub(crate) struct WasmExecutor;

impl WasmExecutor {
    /// Executes non-void function by provided name.
    /// Panics if function is void.
    pub(crate) fn execute(
        ext: Ext,
        program: &Program,
        meta_binary: &[u8],
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        function_name: &str,
    ) -> Result<Vec<u8>> {
        let engine = Engine::default();
        let mut store: Store<HostState<Ext>> = Store::new(&engine, None);

        let mut linker: Linker<HostState<Ext>> = Linker::new();

        let memory_type = MemoryType::new(program.static_pages().0, None);
        let memory = WasmiMemory::new(&mut store, memory_type).map_err(WasmiError::from)?;

        linker
            .define("env", "memory", memory)
            .map_err(WasmiError::from)?;

        let forbidden_funcs = ext.forbidden_funcs().clone();
        let functions = funcs_tree::build(&mut store, memory, forbidden_funcs);
        for (name, function) in functions {
            linker
                .define("env", name, function)
                .map_err(WasmiError::from)?;
        }

        let runtime = State {
            ext,
            err: FuncError::Terminated(TerminationReason::Success),
        };

        *store.state_mut() = Some(runtime);

        let module =
            Module::new(store.engine(), &mut &meta_binary[..]).map_err(WasmiError::from)?;

        let instance_pre = linker
            .instantiate(&mut store, &module)
            .map_err(WasmiError::from)?;

        let instance = instance_pre
            .ensure_no_start(&mut store)
            .map_err(WasmiError::from)?;

        let _gear_gas = instance
            .get_export(&store, GLOBAL_NAME_GAS)
            .and_then(Extern::into_global)
            .and_then(|g| {
                g.set(&mut store, Value::I64(u64::MAX as i64))
                    .map(|_| g)
                    .ok()
            })
            .ok_or(TestError::Instrumentation)?;

        let _gear_allowance = instance
            .get_export(&store, GLOBAL_NAME_ALLOWANCE)
            .and_then(Extern::into_global)
            .and_then(|g| {
                g.set(&mut store, Value::I64(u64::MAX as i64))
                    .map(|_| g)
                    .ok()
            })
            .ok_or(TestError::Instrumentation)?;

        let mut memory_wrap = MemoryWrap::new(memory, store);
        Self::set_pages(&mut memory_wrap, memory_pages)?;

        let res = {
            let func = match instance
                .get_export(&memory_wrap.store, function_name)
                .and_then(Extern::into_func)
            {
                Some(f) => f,
                None => {
                    return Err(TestError::FunctionNotFound(function_name.to_string()));
                }
            };

            let entry_func = match func.typed::<(), (i32,), _>(&mut memory_wrap.store) {
                Ok(f) => f,
                Err(e) => {
                    return Err(e.into());
                }
            };

            entry_func.call(&mut memory_wrap.store, ())
        };

        match res {
            Ok((ptr_to_result,)) => Self::read_result(&memory_wrap, ptr_to_result),
            Err(_) => {
                if let Some(processor_error) = memory_wrap
                    .store
                    .state()
                    .as_ref()
                    .unwrap()
                    .ext
                    .error_explanation
                    .clone()
                {
                    Err(processor_error.into())
                } else {
                    Err(TestError::InvalidReturnType)
                }
            }
        }
    }

    pub(crate) fn update_ext(ext: &mut Ext, manager: &ExtManager) {
        ext.context.block_info.height = manager.block_info.height;
        ext.context.block_info.timestamp = manager.block_info.timestamp;
    }

    pub(crate) fn build_ext(program: &Program, payload: Payload) -> Ext {
        let message =
            IncomingMessage::new(Default::default(), Default::default(), payload, 0, 0, None);
        Ext::new(ProcessorContext {
            gas_counter: GasCounter::new(u64::MAX),
            gas_allowance_counter: GasAllowanceCounter::new(u64::MAX),
            gas_reserver: GasReserver::new(message.id(), 0, Default::default(), MAX_RESERVATIONS),
            system_reservation: None,
            value_counter: ValueCounter::new(u128::MAX),
            allocations_context: AllocationsContext::new(
                program.allocations().clone(),
                program.static_pages(),
                WasmPageNumber(512u32),
            ),
            message_context: MessageContext::new(
                message,
                program.id(),
                None,
                ContextSettings::new(
                    WRITE_COST * 2,
                    WRITE_COST * 4,
                    WRITE_COST * 3,
                    WRITE_COST * 2,
                    WRITE_COST * 2,
                    1024,
                ),
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
            waitlist_cost: WAITLIST_COST,
            reserve_for: RESERVE_FOR,
            reservation: RESERVATION_COST,
            random_data: ([0u8; 32].to_vec(), 0),
        })
    }

    fn read_result(memory: &MemoryWrap<Ext>, ptr_to_result_data: i32) -> Result<Vec<u8>> {
        let offset = ptr_to_result_data as usize;

        // Reading a fat pointer from the `offset`
        let mut ptr = [0u8; mem::size_of::<i32>()];
        let mut len = [0u8; mem::size_of::<i32>()];

        memory.read(offset, &mut ptr)?;

        memory.read(offset + ptr.len(), &mut len)?;

        let ptr = i32::from_ne_bytes(ptr) as usize;
        let len = i32::from_ne_bytes(len) as usize;

        // Reading a vector from `ptr`
        let mut result = vec![0; len];

        memory.read(ptr, &mut result)?;

        Ok(result)
    }

    fn set_pages(
        memory: &mut MemoryWrap<Ext>,
        pages: &BTreeMap<PageNumber, Box<PageBuf>>,
    ) -> Result<()> {
        let memory_size = WasmPageNumber(memory.size().0);
        for (page_number, buffer) in pages {
            let wasm_page_number = page_number.to_wasm_page();
            if memory_size <= wasm_page_number {
                return Err(TestError::InsufficientMemory(memory_size, wasm_page_number));
            }

            memory
                .write(page_number.offset(), &buffer[..])
                .map_err(|_| TestError::InsufficientMemory(memory_size, wasm_page_number))?
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
            println!("{err:?}");
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

    #[test]
    fn meta_block_timestamp() {
        let system = System::default();
        let start_timestamp = system.block_timestamp();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_block_info.wasm",
        );

        let timestamp = u64::from_le_bytes(program.meta_state_empty().unwrap());
        assert_eq!(start_timestamp, timestamp);

        system.spend_blocks(42);
        assert_eq!(system.block_height(), 42);

        let timestamp = u64::from_le_bytes(program.meta_state_empty().unwrap());
        assert_eq!(system.block_timestamp(), timestamp);
        assert_eq!(start_timestamp + 42 * 1000, timestamp);
    }
}
