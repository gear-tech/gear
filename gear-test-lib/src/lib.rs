use std::{path::Path, fs};
use gear_core::message::Message;
use gear_core::program::{Program as CoreProgram, ProgramId};
use codec::{Decode, Encode};

pub trait Program {
    type StateArgument: Encode;
    type State: Decode + Encode;

    fn process_message<T: Into<Message>>(message: T);
    fn query_state(arg: Self::StateArgument) -> Self::State;
}

pub trait WasmModule {
    fn init();
    fn handle();
    fn handle_reply();
    fn state();
}

pub struct MockFromWasm(CoreProgram);

impl MockFromWasm {
    pub fn new(id: ProgramId, code: Vec<u8>) -> anyhow::Result<Self> {
        CoreProgram::new(id, code).map(|v| Self(v))
    }

    pub fn from_file<P: AsRef<Path>>(id: ProgramId, path: P) -> anyhow::Result<Self> {
        let code = fs::read(&path)?;

        Self::new(id, code)
    }
}
