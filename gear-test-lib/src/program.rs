use crate::{
    manager::{ExtManager, Program as InnerProgram, ProgramState},
    system::System,
};
use codec::Encode;
use gear_core::{
    message::{Message, MessageId},
    program::{Program as CoreProgram, ProgramId},
};
use std::{fmt::Debug, fs, path::Path, sync::Mutex};

pub trait WasmProgram: Debug {
    fn init(&mut self, payload: Vec<u8>) -> Result<Vec<u8>, &'static str>;
    fn handle(&mut self, payload: Vec<u8>) -> Result<Vec<u8>, &'static str>;
    fn handle_reply(&mut self, payload: Vec<u8>) -> Result<Vec<u8>, &'static str>;
}

pub struct Program<'a> {
    manager: &'a Mutex<ExtManager>,
    id: ProgramId,
}

impl<'a> Program<'a> {
    fn program_with_id(system: &'a System, id: u64, program: InnerProgram) -> Self {
        let program_id = ProgramId::from(id);

        if system
            .0
            .lock()
            .unwrap()
            .programs
            .insert(program_id, (program, ProgramState::Uninitialized(None)))
            .is_some()
        {
            panic!(
                "Can't create program with id {:?}, because Program with this id already exists",
                id
            )
        }

        Self {
            manager: &system.0,
            id: program_id,
        }
    }

    pub fn mock<T: WasmProgram + 'static>(system: &'a System, mock: T) -> Self {
        let nonce = system.0.lock().unwrap().free_id_nonce();

        Self::mock_with_id(system, nonce, mock)
    }

    pub fn mock_with_id<T: WasmProgram + 'static>(system: &'a System, id: u64, mock: T) -> Self {
        Self::program_with_id(system, id, InnerProgram::Mock(Box::new(mock)))
    }

    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        let nonce = system.0.lock().unwrap().free_id_nonce();

        Self::from_file_with_id(system, nonce, path)
    }

    pub fn from_file_with_id<P: AsRef<Path>>(system: &'a System, id: u64, path: P) -> Self {
        let code =
            fs::read(&path).unwrap_or_else(|_| panic!("Failed to find file {:?}", path.as_ref()));

        let program = CoreProgram::new(ProgramId::from(id), code)
            .expect("Failed to create Program from code");

        Self::program_with_id(system, id, InnerProgram::Core(program))
    }

    pub fn send<E: Encode>(&self, payload: E) {
        self.send_bytes(payload.encode())
    }

    pub fn send_bytes<T: AsRef<[u8]>>(&self, payload: T) {
        let mut system = self.manager.lock().unwrap();

        let message = Message::new(
            MessageId::from(system.fetch_inc_message_nonce()),
            ProgramId::from(system.user),
            self.id,
            payload.as_ref().to_vec().into(),
            u64::MAX,
            0,
        );

        system.run_message(message)
    }
}
