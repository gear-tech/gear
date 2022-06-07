use core_processor::common::JournalNote;
use gear_backend_wasmtime::{env::StoreData};
use gear_core::{
    env::{Ext, ExtCarrier},
    ext::ExtImplementedStruct,
    ids::ProgramId,
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::IncomingMessage,
    program::Program,
};
use std::{collections::BTreeMap, convert::TryInto};
use gear_backend_wasmtime::funcs::FuncsHandler;
use wasmtime::{
    Config, Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store,
    Val
};

pub struct WasmExecutor {
    instance: Instance,
    store: Store<StoreData<ExtImplementedStruct>>,
    memory: WasmtimeMemory,
    program_id: ProgramId,
}

impl WasmExecutor {
    pub(crate) fn new(
        source: ProgramId,
        program: Program,
        memory_pages: &BTreeMap<PageNumber, Box<PageBuf>>,
        message: Option<IncomingMessage>,
    ) -> Self {
        let ext = ExtImplementedStruct::new_(source, program.id(), message);
        let ext_carrier = ExtCarrier::new(ext);
        let store_data = StoreData {
            ext: ext_carrier.cloned(),
            termination_reason: None,
        };

        let config = Config::new();
        let engine = Engine::new(&config).expect("Failed to create engine");
        let mut store = Store::<StoreData<ExtImplementedStruct>>::new(&engine, store_data);

        let module = Module::new(&engine, program.code().code()).expect("Failed to create module");

        let mut memory =
            WasmtimeMemory::new(&mut store, MemoryType::new(program.static_pages().0, None))
                .expect("Failed to create memory");

        let mut funcs = BTreeMap::<&'static str, Func>::new();
        funcs.insert("alloc", FuncsHandler::alloc(&mut store, memory));
        funcs.insert("free", FuncsHandler::free(&mut store));
        funcs.insert("gas", FuncsHandler::gas(&mut store));
        funcs.insert("gr_block_height", FuncsHandler::block_height(&mut store));
        funcs.insert(
            "gr_block_timestamp",
            FuncsHandler::block_timestamp(&mut store),
        );
        funcs.insert(
            "gr_create_program_wgas",
            FuncsHandler::create_program_wgas(&mut store, memory),
        );
        funcs.insert("gr_exit_code", FuncsHandler::exit_code(&mut store));
        funcs.insert("gr_gas_available", FuncsHandler::gas_available(&mut store));
        funcs.insert("gr_debug", FuncsHandler::debug(&mut store, memory));
        funcs.insert("gr_exit", FuncsHandler::exit(&mut store, memory));
        funcs.insert("gr_origin", FuncsHandler::origin(&mut store, memory));
        funcs.insert("gr_msg_id", FuncsHandler::msg_id(&mut store, memory));
        funcs.insert(
            "gr_program_id",
            FuncsHandler::program_id(&mut store, memory),
        );
        funcs.insert("gr_read", FuncsHandler::read(&mut store, memory));
        funcs.insert("gr_reply", FuncsHandler::reply(&mut store, memory));
        funcs.insert(
            "gr_reply_wgas",
            FuncsHandler::reply_wgas(&mut store, memory),
        );
        funcs.insert(
            "gr_reply_commit",
            FuncsHandler::reply_commit(&mut store, memory),
        );
        funcs.insert(
            "gr_reply_commit_wgas",
            FuncsHandler::reply_commit_wgas(&mut store, memory),
        );
        funcs.insert(
            "gr_reply_push",
            FuncsHandler::reply_push(&mut store, memory),
        );
        funcs.insert("gr_reply_to", FuncsHandler::reply_to(&mut store, memory));
        funcs.insert("gr_send_wgas", FuncsHandler::send_wgas(&mut store, memory));
        funcs.insert("gr_send", FuncsHandler::send(&mut store, memory));
        funcs.insert(
            "gr_send_commit_wgas",
            FuncsHandler::send_commit_wgas(&mut store, memory),
        );
        funcs.insert(
            "gr_send_commit",
            FuncsHandler::send_commit(&mut store, memory),
        );
        funcs.insert("gr_send_init", FuncsHandler::send_init(&mut store, memory));
        funcs.insert("gr_send_push", FuncsHandler::send_push(&mut store, memory));
        funcs.insert("gr_size", FuncsHandler::size(&mut store));
        funcs.insert("gr_source", FuncsHandler::source(&mut store, memory));
        funcs.insert("gr_value", FuncsHandler::value(&mut store, memory));
        funcs.insert(
            "gr_value_available",
            FuncsHandler::value_available(&mut store, memory),
        );
        funcs.insert("gr_leave", FuncsHandler::leave(&mut store));
        funcs.insert("gr_wait", FuncsHandler::wait(&mut store));
        funcs.insert("gr_wake", FuncsHandler::wake(&mut store, memory));

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

    pub(crate) fn call_function(&mut self, function_name: &str) -> (Vec<u8>, Vec<JournalNote>) {
        let function = self.get_function(function_name);
        let mut prt_to_result_array = [Val::I32(0)];

        function
            .call(&mut self.store, &[], &mut prt_to_result_array)
            .expect("Failed call");

        let execution_result = match prt_to_result_array[0] {
            Val::I32(ptr_to_result) => self.read_result(ptr_to_result.clone()),
            _ => {
                panic!("{}", "Got wrong type")
            }
        };
        (execution_result, self.get_memory_updates())
    }

    pub(crate) fn call_void_function(&mut self, function_name: &str) -> Vec<JournalNote> {
        let function = self.get_function(function_name);
        function
            .call(&mut self.store, &[], &mut [])
            .expect("Failed call");

        self.get_memory_updates()
    }

    fn get_memory_updates(&mut self) -> Vec<JournalNote> {
        let mut pages_data = BTreeMap::new();
        let mut buffer = [0u8; PageNumber::size()];
        let memory_size = self.memory.data_size(&mut self.store);
        for page_number in (0..memory_size).step_by(PageNumber::size()) {
            if let Err(err) = self.memory.read(&mut self.store, page_number, &mut buffer) {
                panic!("{:?}", err.to_string())
            }
            pages_data.insert(PageNumber::new_from_addr(page_number), buffer.to_vec());
        }
        self.build_jornal_notes(pages_data)
    }

    fn build_jornal_notes(&self, pages_data: BTreeMap<PageNumber, Vec<u8>>) -> Vec<JournalNote> {
        pages_data
            .into_iter()
            .map(|(page_number, data)| JournalNote::UpdatePage {
                program_id: self.program_id,
                page_number,
                data,
            })
            .collect()
    }

    fn get_function(&mut self, function_name: &str) -> Func {
        self.instance
            .get_func(&mut self.store, function_name)
            .expect("No function with such name")
    }

    fn read_result(&mut self, ptr_to_result_data: i32) -> Vec<u8> {
        let mut ptr_to_result_buffer: Vec<u8> = Vec::new();
        let buffer_size = ptr_to_result_data.to_be_bytes().len();
        ptr_to_result_buffer.resize(buffer_size, 0);

        self.memory
            .read(
                &self.store,
                ptr_to_result_data as usize,
                &mut ptr_to_result_buffer,
            )
            .expect("Failed to read data ptr");

        let mut result_len_buffer = [0u8];

        self.memory
            .read(
                &self.store,
                ptr_to_result_data as usize + buffer_size,
                &mut result_len_buffer,
            )
            .expect("Failed to read data length");

        let decoded_ptr_to_result =
            i32::from_ne_bytes(ptr_to_result_buffer.try_into().unwrap()) as usize;
        let result_len = result_len_buffer[0] as usize;

        let mut results = Vec::new();
        results.resize(result_len, 0u8);

        self.memory
            .read(&self.store, decoded_ptr_to_result, &mut results)
            .expect("Failed to read result");

        results
    }

    fn set_pages<T: Ext>(
        mut store: &mut Store<StoreData<T>>,
        memory: &mut WasmtimeMemory,
        pages: &BTreeMap<PageNumber, Box<PageBuf>>,
    ) -> Result<(), String> {
        let memory_size = WasmPageNumber(memory.size(&mut store) as u32);
        for (page_number, buffer) in pages {
            if memory_size <= page_number.to_wasm_page() {
                panic!("Memory size {:?} less then {:?}", memory_size, page_number);
            }
            memory
                .write(&mut store, page_number.offset(), &buffer[..])
                .map_err(|error| format!("Cannot write to {:?}: {:?}", page_number, error))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Program, System};
    use codec::{Decode, Encode};
    use gear_core::{ids::ProgramId, message::IncomingMessage};

    #[derive(Clone, Debug, Decode, Encode, PartialEq)]
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
    fn test_meta_state_function() {
        let meta_state_function_name = "meta_state";
        let init_function_name = "init";
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

        let init_message = IncomingMessage::new(
            system.0.borrow_mut().fetch_inc_message_nonce().into(),
            user_id,
            MessageInitIn::encode(&MessageInitIn {
                amount: 1,
                currency: "1".to_string(),
            }),
            0,
            0,
            None,
        );

        system.0.borrow_mut().execute_custom_void_function(
            user_id,
            &program.id,
            Some(init_message),
            init_function_name,
        );

        let result = system.0.borrow_mut().execute_custom_function(
            user_id,
            &program.id,
            Some(meta_state_message),
            meta_state_function_name,
        );

        assert_eq!(result, expected_result.encode());
    }
}
