use std::collections::{HashMap, VecDeque};

use crate::{
    memory::{Allocations, PageNumber},
    message::Message,
    program::{ProgramId, Program},
};

pub struct Runner {
    programs: HashMap<ProgramId, Program>,
    allocations: Allocations,
    message_queue: VecDeque<Message>,
    memory: Vec<u8>,
}

impl Runner {
    fn new(
        programs: Vec<Program>,
        allocations: Vec<(PageNumber, ProgramId)>,
        message_queue: Vec<Message>,
        memory: Vec<u8>
    ) -> Self {
        Self {
            programs: programs.into_iter().map(|p| (p.id(), p)).collect(),
            allocations: Allocations::new(allocations),
            message_queue: VecDeque::new(),
            memory,
        }
    }
}
