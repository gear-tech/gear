use std::path::Path;

use codec::{Encode, Decode};

use crate::{
    message::Message,
    program::{Program, ProgramId},
    memory::PageNumber,
    runner::{Runner, Config},
};

#[derive(Decode, Default, Encode, Clone, Debug)]
pub struct State {
    pub programs: Vec<Program>,
    pub queued_messages: Vec<Message>,
    pub memory: Vec<u8>,
    pub allocations: Vec<(PageNumber, ProgramId)>,
}

pub fn load_from_file<P: AsRef<Path>>(path: P) -> State {
    std::fs::read(path).map(|bytes| {
        State::decode(&mut &bytes[..]).expect("Failed to decode binary")
    }).unwrap_or_default()
}

pub fn save_to_file<P: AsRef<Path>>(path: P, state: &State) {
    let mut bytes = vec![];
    state.encode_to(&mut bytes);

    std::fs::write(path, bytes).expect("Failed to write data");
}

impl State {
    pub fn into_runner(self) -> Runner {
        let State { programs, queued_messages, memory, allocations } = self;

        Runner::new(
            &Config::default(),
            programs,
            allocations,
            queued_messages,
            &memory[..],
        )
    }

    pub fn from_runner(runner: Runner) -> Self {
        let (programs, allocations, queued_messages, memory) = runner.complete();
        Self { programs, allocations, queued_messages, memory }
    }
}
