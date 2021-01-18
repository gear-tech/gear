use std::path::Path;

use codec::{Encode, Decode};

use gear_core::{
    message::Message,
    program::{Program, ProgramId},
    memory::PageNumber,
    runner::{Runner, Config},
    storage::{new_in_memory, InMemoryStorage, InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage},
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

type InMemoryRunner = Runner<InMemoryAllocationStorage, InMemoryMessageQueue, InMemoryProgramStorage>;

impl State {
    pub fn into_runner(self) -> InMemoryRunner {
        let State { allocations, queued_messages, programs, memory } = self;

        Runner::new(
            &Config::default(),
            new_in_memory(allocations, queued_messages, programs),
            &memory[..],
        )
    }

    pub fn from_runner(runner: InMemoryRunner) -> Self {
        let ( InMemoryStorage { allocation_storage, message_queue, program_storage } , memory) = runner.complete();
        Self { 
            allocations: allocation_storage.drain(), 
            queued_messages: message_queue.drain(), 
            programs: program_storage.drain(), 
            memory,
        }
    }
}
