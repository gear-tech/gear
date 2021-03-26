use gear_core::{
    storage::{AllocationStorage, ProgramStorage, MessageQueue},
    program::{ProgramId, Program},
    memory::PageNumber,
    message::Message,
};

pub struct ExtAllocationStorage;
pub struct ExtProgramStorage;
pub struct ExtMessageQueue;

impl AllocationStorage for ExtAllocationStorage {
    fn get(&self, id: PageNumber) -> Option<ProgramId> {
        gear_runtime::ext::page_info(id.raw())
    }

    fn remove(&mut self, id: PageNumber) -> Option<ProgramId> {
        gear_runtime::ext::dealloc(id.raw());
        None
    }

    fn set(&mut self, page: PageNumber, program: ProgramId) {
        gear_runtime::ext::alloc(page.raw(), program);
    }

    fn clear(&mut self, _program_id: ProgramId) {
        unimplemented!()
    }
}

impl ProgramStorage for ExtProgramStorage {
    fn get(&self, id: ProgramId) -> Option<Program> {
        gear_runtime::ext::get_program(id)
    }

    fn set(&mut self, program: Program) -> Option<Program> {
        gear_runtime::ext::set_program(program);
        None
    }

    fn remove(&mut self, id: ProgramId) -> Option<Program> {
        gear_runtime::ext::remove_program(id);
        None
    }
}

impl MessageQueue for ExtMessageQueue {
    fn dequeue(&mut self) -> Option<Message> {
        gear_runtime::ext::dequeue_message()
    }

    fn queue(&mut self, message: Message) {
        gear_runtime::ext::queue_message(message);
    }
}
