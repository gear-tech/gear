use gear_core::{
    storage::{AllocationStorage, ProgramStorage, MessageQueue},
    program::{ProgramId, Program},
    memory::PageNumber,
    message::Message,
};

pub struct ExtAllocationStorage;

pub struct ExtProgramStorage;

#[derive(Default)]
pub struct ExtMessageQueue {
    pub log: Vec<Message>,
}

impl AllocationStorage for ExtAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        gear_common::native::page_info(id.raw())
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        gear_common::native::dealloc(id.raw());
        None
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        gear_common::native::alloc(page.raw(), program)
    }

    fn clear(&mut self, _program_id: ProgramId) {
        unimplemented!()
    }
}

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        gear_common::native::get_program(id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        gear_common::native::set_program(program);
        None
    }

    fn remove(&mut self, _id: ProgramId) -> Option<Program> {
        unimplemented!()
    }
}

impl MessageQueue for ExtMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        gear_common::native::dequeue_message()
    }

    fn queue(&mut self, message: Message) {
        if message.dest == 0.into() {
            self.log.push(message);
            return;
        }

        gear_common::native::queue_message(message)
    }
}
