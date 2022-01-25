use crate::WasmProgram;
use core_processor::{common::*, configs::BlockInfo, Ext};
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program as CoreProgram, ProgramId},
};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Debug)]
pub(crate) enum ProgramState {
    Initialized,
    Uninitialized(Option<MessageId>),
    FailedInitialization,
}

#[derive(Debug)]
pub(crate) enum Program {
    Core(CoreProgram),
    Mock(Box<dyn WasmProgram>),
}

#[derive(Default, Debug)]
pub(crate) struct ExtManager {
    // State metadata
    pub(crate) block_info: BlockInfo,

    // Messaging and programs meta
    pub(crate) msg_nonce: u64,
    pub(crate) id_nonce: u64,
    pub(crate) user: ProgramId,

    // State
    pub(crate) programs: BTreeMap<ProgramId, (Program, ProgramState)>,
    pub(crate) message_queue: VecDeque<Message>,
    pub(crate) mailbox: BTreeMap<ProgramId, Vec<Message>>,
    pub(crate) wait_list: BTreeMap<(ProgramId, MessageId), Message>,
    pub(crate) wait_init_list: BTreeMap<ProgramId, Vec<MessageId>>,

    // Last run info
    pub(crate) log: Vec<Message>,
    pub(crate) failed: bool,
}

impl ExtManager {
    pub(crate) fn new() -> Self {
        Self {
            msg_nonce: 1,
            id_nonce: 1,
            user: 100001.into(),
            ..Default::default()
        }
    }

    pub(crate) fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub(crate) fn free_id_nonce(&mut self) -> u64 {
        while self.programs.contains_key(&self.id_nonce.into()) {
            self.id_nonce += 1;
        }
        self.id_nonce
    }

    pub(crate) fn run_message(&mut self, message: Message) {
        self.clear();

        if self.programs.contains_key(&message.dest()) {
            self.message_queue.push_back(message);
        } else {
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message);
        }

        while let Some(message) = self.message_queue.pop_front() {
            let (prog, state) = self
                .programs
                .get_mut(&message.dest())
                .expect("Somehow message queue contains message for user");

            if let ProgramState::FailedInitialization = state {
                panic!(
                    "Program with id {} failed it's initialization, so can't receive messages",
                    message.dest(),
                )
            }

            let kind = if let Some(kind) = message
                .reply()
                .map(|_| DispatchKind::HandleReply)
                .or_else(|| {
                    if let ProgramState::Uninitialized(message_id) = state {
                        if let Some(id) = message_id {
                            if *id == message.id() {
                                Some(DispatchKind::Init)
                            } else {
                                None
                            }
                        } else {
                            *message_id = Some(message.id());

                            Some(DispatchKind::Init)
                        }
                    } else {
                        Some(DispatchKind::Handle)
                    }
                }) {
                kind
            } else {
                self.wait_init_list
                    .entry(message.dest())
                    .or_default()
                    .push(message.id());

                self.wait_dispatch(Dispatch {
                    kind: DispatchKind::Handle,
                    message,
                });

                continue;
            };

            match prog {
                Program::Core(program) => {
                    let ProcessResult { journal, .. } =
                        core_processor::process::<WasmtimeEnvironment<Ext>>(
                            program.clone(),
                            Dispatch { kind, message },
                            self.block_info,
                        );

                    core_processor::handle_journal(journal, self);
                }
                Program::Mock(mock) => {
                    let payload = message.payload().to_vec();

                    let reply_payload = match kind {
                        DispatchKind::Init => mock.init(payload),
                        DispatchKind::Handle => mock.handle(payload),
                        DispatchKind::HandleReply => mock.handle_reply(payload),
                    };

                    let (bytes, exit_code) = match reply_payload {
                        Ok(payload) => {
                            if let DispatchKind::Init = kind {
                                self.init_success(message.id(), message.dest());
                            }

                            (payload, 0)
                        }
                        Err(trap) => {
                            if let DispatchKind::Init = kind {
                                self.message_dispatched(DispatchOutcome::InitFailure {
                                    message_id: message.id(),
                                    program_id: message.dest(),
                                    origin: message.source(),
                                    reason: "Err(_) returned in function",
                                })
                            } else {
                                self.message_dispatched(DispatchOutcome::MessageTrap {
                                    message_id: message.id(),
                                    program_id: message.dest(),
                                    trap: Some("Err(_) returned in function"),
                                })
                            }

                            (trap.as_bytes().to_vec(), 1)
                        }
                    };

                    let nonce = self.fetch_inc_message_nonce();

                    if exit_code != 0 || !bytes.is_empty() {
                        self.send_message(
                            message.id(),
                            Message::new_reply(
                                nonce.into(),
                                message.dest(),
                                message.source(),
                                bytes.into(),
                                message.gas_limit(),
                                0,
                                message.id(),
                                exit_code,
                            ),
                        );
                    }
                }
            }
        }
    }

    fn clear(&mut self) {
        self.log.clear();
        self.failed = false;

        if !self.message_queue.is_empty() {
            panic!("Message queue isn't empty");
        }
    }

    fn init_success(&mut self, message_id: MessageId, program_id: ProgramId) {
        self.programs
            .get_mut(&program_id)
            .expect("Can't find existing program")
            .1 = ProgramState::Initialized;

        if let Some(ids) = self.wait_init_list.remove(&program_id) {
            for id in ids {
                self.wake_message(message_id, program_id, id);
            }
        }
    }

    fn init_failure(&mut self, program_id: ProgramId) {
        self.programs
            .get_mut(&program_id)
            .expect("Can't find existing program")
            .1 = ProgramState::FailedInitialization
    }
}

impl JournalHandler for ExtManager {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        self.failed = self.failed
            || matches!(
                outcome,
                DispatchOutcome::MessageTrap { .. } | DispatchOutcome::InitFailure { .. }
            );

        match outcome {
            DispatchOutcome::InitSuccess {
                message_id,
                program,
                ..
            } => self.init_success(message_id, program.id()),

            DispatchOutcome::InitFailure { program_id, .. } => self.init_failure(program_id),

            _ => {}
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
            self.mailbox
                .entry(message.dest())
                .or_default()
                .push(message.clone());
            self.log.push(message);
        }
    }
    fn wait_dispatch(&mut self, dispatch: Dispatch) {
        self.message_consumed(dispatch.message.id());
        self.wait_list.insert(
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
    fn update_nonce(&mut self, program_id: ProgramId, nonce: u64) {
        if let Some((Program::Core(prog), _)) = self.programs.get_mut(&program_id) {
            prog.set_message_nonce(nonce);
        } else {
            panic!("Program not found in storage");
        }
    }
    fn update_page(
        &mut self,
        program_id: ProgramId,
        page_number: PageNumber,
        data: Option<Vec<u8>>,
    ) {
        if let Some((Program::Core(prog), _)) = self.programs.get_mut(&program_id) {
            if let Some(data) = data {
                let _ = prog.set_page(page_number, &data);
            } else {
                prog.remove_page(page_number);
            }
        } else {
            panic!("Program not found in storage");
        }
    }
}
