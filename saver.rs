use std::path::Path;

use codec::{Encode, Decode};

use crate::message::Message;
use crate::program::{Program, ProgramId};
use crate::memory::PageNumber;

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
