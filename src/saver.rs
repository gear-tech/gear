use std::path::Path;

use codec::{Encode, Decode};

use crate::message::Message;
use crate::program::{Program, ProgramId};
use crate::memory::PageNumber;
use crate::runner::Runner;

#[derive(Decode, Default, Encode, Clone, Debug)]
struct State {
    programs: Vec<Program>,
    queued_messages: Vec<Message>,
    memory: Vec<u8>,
    allocations: Vec<(PageNumber, ProgramId)>,
}

fn load_from_file<P: AsRef<Path>>(path: P) -> State {
    std::fs::read(path).map(|bytes| {
        State::decode(&mut &bytes[..]).expect("Failed to decode binary")
    }).unwrap_or_default()
}

fn save_to_file<P: AsRef<Path>>(path: P, state: &State) {
    let mut bytes = vec![];
    state.encode_to(&mut bytes);

    std::fs::write(path, bytes).expect("Failed to write data");
}

impl State {

    fn to_runner(self) -> Runner {
        let State { programs, queued_messages, memory, allocations } = self;

        Runner::new(
            programs,
            allocations,
            queued_messages,
            memory,
        )
    }

    fn from_runner(runner: Runner) -> Self {
        let Runner { mut programs, allocations, message_queue, memory } = runner;
        Self {
            programs: programs.drain().map(|(_, v)| v).collect(),
            queued_messages: message_queue.into_iter().collect(),
            memory,
            allocations: allocations.drain(),
        }
    }

}
