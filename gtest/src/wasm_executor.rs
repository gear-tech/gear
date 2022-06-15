use codec::Encode;
use core_processor::{Ext, ProcessorExt};
use gear_backend_wasmtime::{env::StoreData, funcs_tree};
use gear_core::{
    env::{Ext as ExtTrait, ExtCarrier},
    gas::{GasAllowanceCounter, GasCounter, ValueCounter},
    ids::ProgramId,
    memory::{AllocationsContext, PageBuf, PageNumber, WasmPageNumber},
    message::{IncomingMessage, MessageContext},
    program::Program,
};
use std::{collections::BTreeMap, mem};
use wasmtime::{
    Config, Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store,
    Val,
};

/// Binary meta-functions executor for testing purposes
pub struct WasmExecutor {
    instance: Instance,
    store: Store<StoreData<Ext>>,
    memory: WasmtimeMemory,
    program_id: ProgramId,
}

impl WasmExecutor {
    /// Creates a WasmExecutor instance from a program.
    /// Also uses provided memory pages for future execution
    pub(crate) fn new(
        source: ProgramId,
        program: &Program,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        message: Option<IncomingMessage>,
    ) -> Self {
        let ext = WasmExecutor::build_ext(source, program, message);
        let ext_carrier = ExtCarrier::new(ext);
        let store_data = StoreData {
            ext: ext_carrier.cloned(),
            termination_reason: None,
        };

        let config = Config::new();
        let engine = Engine::new(&config).expect("Failed to create engine");
        let mut store = Store::<StoreData<Ext>>::new(&engine, store_data);

        let module = Module::new(&engine, program.code().code()).expect("Failed to create module");

        let mut memory =
            WasmtimeMemory::new(&mut store, MemoryType::new(program.static_pages().0, None))
                .expect("Failed to create memory");
        let funcs = funcs_tree::build(&mut store, memory, None);
        let mut externs = Vec::with_capacity(module.imports().len());
        for import in module.imports() {
            if import.module() != "env" {
                panic!("Non environment import in module");
            }
            match import.name() {
                Some("memory") => externs.push(Extern::Memory(memory)),
                Some(key) => {
                    if funcs.contains_key(key) {
                        externs.push(funcs[key].into())
                    } else {
                        panic!("Wasm is asking for unknown function: {:?}. Consider to add in from FuncsHandler", key)
                    }
                }
                _ => continue,
            };
        }

        let instance =
            Instance::new(&mut store, &module, &externs).expect("Failed to create instance");
        WasmExecutor::set_pages(&mut store, &mut memory, memory_pages)
            .expect("Failed to set memory pages");

        Self {
            instance,
            store,
            memory,
            program_id: program.id(),
        }
    }

    /// Executes non-void function by provided name.
    /// Panics if no function with such name was found or function was void
    pub(crate) fn execute(&mut self, function_name: &str) -> Vec<u8> {
        let function = self.get_function(function_name);
        let mut prt_to_result_array = [Val::I32(0)];

        function
            .call(&mut self.store, &[], &mut prt_to_result_array)
            .expect("Failed call");

        match prt_to_result_array[0] {
            Val::I32(ptr_to_result) => self.read_result(ptr_to_result),
            _ => panic!("{}", "Got wrong type"),
        }
    }

    fn build_ext(source: ProgramId, program: &Program, message: Option<IncomingMessage>) -> Ext {
        core_processor::Ext::new(
            GasCounter::new(u64::MAX),
            GasAllowanceCounter::new(u64::MAX),
            ValueCounter::new(u128::MAX),
            AllocationsContext::new(
                program.get_allocations().clone(),
                program.static_pages(),
                WasmPageNumber(512u32),
            ),
            MessageContext::new(
                message.unwrap_or_else(|| {
                    IncomingMessage::new(
                        Default::default(),
                        source,
                        Option::<bool>::encode(&None),
                        0,
                        0,
                        None,
                    )
                }),
                program.id(),
                None,
            ),
            Default::default(),
            Default::default(),
            Default::default(),
            None,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        )
    }

    fn get_function(&mut self, function_name: &str) -> Func {
        self.instance
            .get_func(&mut self.store, function_name)
            .expect("No function with such name was found")
    }

    fn read_result(&mut self, ptr_to_result_data: i32) -> Vec<u8> {
        let offset = ptr_to_result_data as usize;

        // Reading a fat pointer from the `offset`
        let mut ptr = [0_u8; mem::size_of::<i32>()];
        let mut len = [0_u8; mem::size_of::<i32>()];

        self.memory
            .read(&self.store, offset, &mut ptr)
            .expect("Failed to read data ptr");

        self.memory
            .read(&self.store, offset + ptr.len(), &mut len)
            .expect("Failed to read data length");

        let ptr = i32::from_ne_bytes(ptr) as usize;
        let len = i32::from_ne_bytes(len) as usize;

        // Reading a vector from `ptr`
        let mut result = vec![0; len];

        self.memory
            .read(&self.store, ptr, &mut result)
            .expect("Failed to read result");

        result
    }

    fn set_pages<T: ExtTrait>(
        mut store: &mut Store<StoreData<T>>,
        memory: &mut WasmtimeMemory,
        pages: &BTreeMap<PageNumber, Box<PageBuf>>,
    ) -> Result<(), String> {
        let memory_size = WasmPageNumber(memory.size(&mut store) as u32);
        for (page_number, buffer) in pages {
            if memory_size <= page_number.to_wasm_page() {
                panic!("Memory size {:?} less than {:?}", memory_size, page_number);
            }
            memory
                .write(&mut store, page_number.offset(), &buffer[..])
                .map_err(|error| format!("Cannot write to {:?}: {:?}", page_number, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod meta_tests {
    use crate::{Program, System};
    use codec::{Decode, Encode};
    use gear_core::{ids::ProgramId, message::IncomingMessage};

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
        let meta_state_function_name = "meta_state";
        let user_id: ProgramId = 100.into();
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let meta_state_message = IncomingMessage::new(
            system.0.borrow_mut().fetch_inc_message_nonce().into(),
            user_id,
            Option::<Id>::encode(&Some(Id {
                decimal: 2,
                hex: vec![2u8],
            })),
            0,
            0,
            None,
        );

        let result = system.0.borrow_mut().call_meta(
            user_id,
            &program.id,
            Some(meta_state_message),
            meta_state_function_name,
        );

        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_manager_executions_coworking() {
        let meta_state_function_name = "meta_state";
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

        let meta_state_message = IncomingMessage::new(
            system.0.borrow_mut().fetch_inc_message_nonce().into(),
            user_id,
            Option::<Id>::encode(&expected_id),
            0,
            0,
            None,
        );

        let run_result = program.send(
            user_id,
            MessageInitIn {
                amount: 1,
                currency: "1".to_string(),
            },
        );
        assert!(!run_result.main_failed);
        let result = system.0.borrow_mut().call_meta(
            user_id,
            &program.id,
            Some(meta_state_message),
            meta_state_function_name,
        );

        assert_eq!(result, expected_result.encode());
    }

    #[test]
    #[should_panic(
        expected = "Source id in message is not equal to source id in function arguments"
    )]
    fn fool_test_for_correct_input() {
        let meta_state_function_name = "meta_state";
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        let meta_state_message = IncomingMessage::new(
            system.0.borrow_mut().fetch_inc_message_nonce().into(),
            100.into(),
            Option::<Id>::encode(&Some(Id {
                decimal: 2,
                hex: vec![2u8],
            })),
            0,
            0,
            None,
        );

        system.0.borrow_mut().call_meta(
            200.into(),
            &program.id,
            Some(meta_state_message),
            meta_state_function_name,
        );
    }

    #[test]
    #[should_panic(expected = "No function with such name was found")]
    fn test_failing_with_unknown_function() {
        let unknown_function_name = "fsd314f";
        let user_id: ProgramId = 100.into();
        let system = System::default();
        let program = Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        system
            .0
            .borrow_mut()
            .call_meta(user_id, &program.id, None, unknown_function_name);
    }

    #[test]
    #[should_panic(expected = "No program with such id")]
    fn test_failing_with_unknown_program_id() {
        let meta_state_function_name = "meta_state";
        let user_id: ProgramId = 100.into();
        let system = System::default();
        Program::from_file(
            &system,
            "../target/wasm32-unknown-unknown/release/demo_meta.wasm",
        );

        system
            .0
            .borrow_mut()
            .call_meta(user_id, &0.into(), None, meta_state_function_name);
    }
}
