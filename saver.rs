use codec::{Encode, Decode};

use crate::message::Message;
use crate::program::{Program, ProgramId};
use crate::memory::PageNumber;

#[derive(Decode, Encode, Clone, Debug)]
struct State {
    programs: Vec<Program>,
    queued_messages: Vec<Message>,
    memory: Vec<u8>,
    allocations: Vec<(PageNumber, ProgramId)>,
}

