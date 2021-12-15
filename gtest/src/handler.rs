use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use gear_core_processor::common::*;
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Default)]
pub struct InMemoryHandler {
    message_queue: VecDeque<Message>,
    log: Vec<Message>,
    programs: BTreeMap<ProgramId, Program>,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,
    current_failed: bool,
}

impl CollectState for InMemoryHandler {
    fn collect(&self) -> State {
        let InMemoryHandler {
            message_queue,
            log,
            programs,
            current_failed,
            ..
        } = self.clone();

        State {
            message_queue,
            log,
            programs,
            current_failed,
        }
    }
}

impl JournalHandler for InMemoryHandler {
    fn execution_fail(
        &mut self,
        origin: MessageId,
        _initiator: ProgramId,
        _program_id: ProgramId,
        _reason: &'static str,
        _entry: DispatchKind,
    ) {
        self.message_consumed(origin);
        self.current_failed = true;
    }
    fn gas_burned(&mut self, _origin: MessageId, _amount: u64) {}
    fn message_consumed(&mut self, message_id: MessageId) {
        self.current_failed = false;
        if let Some(index) = self
            .message_queue
            .iter()
            .position(|msg| msg.id() == message_id)
        {
            self.message_queue.remove(index);
        }
    }
    fn message_trap(&mut self, _origin: MessageId, _trap: Option<&'static str>) {}
    fn send_message(&mut self, _origin: MessageId, message: Message) {
        if self.programs.contains_key(&message.dest()) {
            self.message_queue.push_back(message);
        } else {
            self.log.push(message);
        }
    }
    fn submit_program(&mut self, _origin: MessageId, _owner: ProgramId, program: Program) {
        let _ = self.programs.insert(program.id(), program);
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        let _ = self.message_queue.pop_front();
        let _ = self.wait_list.insert(
            (dispatch.message.dest(), dispatch.message.id()),
            dispatch.message,
        );
    }
    fn wake_message(&mut self, _origin: MessageId, program_id: ProgramId, message_id: MessageId) {
        if let Some(msg) = self.wait_list.remove(&(program_id, message_id)) {
            self.message_queue.push_back(msg);
        }
    }
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        if let Some(prog) = self.programs.get_mut(&program_id) {
            prog.set_message_nonce(nonce);
        } else {
            panic!("Program not found in storage");
        }
    }
    fn update_page(&mut self, program_id: ProgramId, page_number: PageNumber, data: Vec<u8>) {
        if let Some(prog) = self.programs.get_mut(&program_id) {
            let _ = prog.set_page(page_number, &data);
        } else {
            panic!("Program not found in storage");
        }
    }
}
