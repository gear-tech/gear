use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use gear_core_processor::common::*;
use std::collections::{BTreeMap, VecDeque, BTreeSet};

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
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        match outcome {
            DispatchOutcome::Success(_) => {
                self.current_failed = false;
            }
            DispatchOutcome::MessageTrap { .. } => {
                self.current_failed = true;
            }
            DispatchOutcome::InitSuccess { program, .. } => {
                self.current_failed = false;
                let _ = self.programs.insert(program.id(), program);
            }
            DispatchOutcome::InitFailure { .. } => {
                self.current_failed = true;
            }
        };
    }
    fn gas_burned(&mut self, _message_id: MessageId, _origin: ProgramId, _amount: u64) {}
    fn message_consumed(&mut self, message_id: MessageId) {
        if let Some(index) = self
            .message_queue
            .iter()
            .position(|msg| msg.id() == message_id)
        {
            self.message_queue.remove(index);
        }
    }
    fn send_message(&mut self, _message_id: MessageId, message: Message) {
        if self.programs.contains_key(&message.dest()) {
            self.message_queue.push_back(message);
        } else {
            self.log.push(message);
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.message_consumed(dispatch.message.id());
        let _ = self.wait_list.insert(
            (dispatch.message.dest(), dispatch.message.id()),
            dispatch.message,
        );
    }
    fn wake_message(
        &mut self,
        _message_id: MessageId,
        program_id: ProgramId,
        awakening_id: MessageId,
    ) {
        if let Some(msg) = self.wait_list.remove(&(program_id, awakening_id)) {
            self.message_queue.push_back(msg);
        }
    }
    fn update_nonce_and_pages_amount(&mut self, program_id: ProgramId, _persistent_pages: BTreeSet<u32>, nonce: u64) {
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
