use crate::empty_ext::ExtImplementedStruct;
use core_processor::common::JournalNote;
use gear_backend_wasmtime::{env::StoreData, funcs_tree};
use gear_core::{
    env::{Ext, ExtCarrier},
    ids::ProgramId,
    memory::{PageBuf, PageNumber, WasmPageNumber},
    message::IncomingMessage,
    program::Program,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    mem,
};
use wasmtime::{
    Config, Engine, Extern, Func, Instance, Memory as WasmtimeMemory, MemoryType, Module, Store,
    Val,
};

#[allow(dead_code)]
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
        let ext = ExtImplementedStruct::new(source, program.id(), message);
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
        let funcs = funcs_tree::build(&mut store, memory, BTreeSet::new());
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

    #[allow(dead_code)]
    pub(crate) fn execute_with_result(
        &mut self,
        function_name: &str,
    ) -> (Vec<u8>, Vec<JournalNote>) {
        let function = self.get_function(function_name);
        let mut prt_to_result_array = [Val::I32(0)];

        function
            .call(&mut self.store, &[], &mut prt_to_result_array)
            .expect("Failed call");

        let execution_result = match prt_to_result_array[0] {
            Val::I32(ptr_to_result) => self.read_result(ptr_to_result),
            _ => {
                panic!("{}", "Got wrong type")
            }
        };
        (execution_result, self.get_memory_updates())
    }

    #[allow(dead_code)]
    pub(crate) fn execute(&mut self, function_name: &str) -> Vec<JournalNote> {
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
                data: PageBuf::new_from_vec(data).expect("Failed to convert data to page buffer"),
            })
            .collect()
    }

    fn get_function(&mut self, function_name: &str) -> Func {
        self.instance
            .get_func(&mut self.store, function_name)
            .expect("No function with such name")
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

    fn set_pages<T: Ext>(
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
mod tests {
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
