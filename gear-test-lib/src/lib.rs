use codec::Encode;
use core_processor::{common::*, configs::BlockInfo, Ext};
use gear_backend_wasmtime::WasmtimeEnvironment;
use gear_core::{
    memory::PageNumber,
    message::{Message, MessageId},
    program::{Program, ProgramId},
};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::Path,
    sync::Mutex,
};

pub mod mock;

pub struct TestSystem(Mutex<System>);

#[derive(Default)]
struct System {
    // State metadata
    block_info: BlockInfo,

    // Messaging and programs meta
    msg_nonce: u64,
    id_nonce: u64,
    user: u64,

    // State
    programs: BTreeMap<ProgramId, Program>,
    message_queue: VecDeque<Message>,
    mailbox: BTreeMap<ProgramId, Vec<Message>>,
    wait_list: BTreeMap<(ProgramId, MessageId), Message>,

    // Last run info
    log: Vec<Message>,
    failed: bool,
}

impl Default for TestSystem {
    fn default() -> Self {
        Self(Mutex::new(System {
            msg_nonce: 1,
            id_nonce: 1,
            user: 100001,
            ..Default::default()
        }))
    }
}

impl System {
    pub fn clear(&mut self) {
        self.log.clear();
        self.failed = false;

        if !self.message_queue.is_empty() {
            panic!("Message queue wasn't empty");
        }
    }

    pub fn fetch_inc_message_nonce(&mut self) -> u64 {
        let nonce = self.msg_nonce;
        self.msg_nonce += 1;
        nonce
    }

    pub fn fetch_inc_id_nonce(&mut self) -> u64 {
        let nonce = self.id_nonce;
        self.id_nonce += 1;
        while self.programs.contains_key(&self.id_nonce.into()) {
            self.id_nonce += 1;
        }
        nonce
    }

    pub fn send_message(&mut self, message: Message) {
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
            let program = self
                .programs
                .get(&message.dest())
                .expect("Somehow message queue contains message for user");

            let kind = if message.reply().is_none() {
                if program.get_pages().is_empty() {
                    DispatchKind::Init
                } else {
                    DispatchKind::Handle
                }
            } else {
                DispatchKind::HandleReply
            };

            let dispatch = Dispatch { kind, message };

            let ProcessResult { journal, .. } = core_processor::process::<WasmtimeEnvironment<Ext>>(
                program.clone(),
                dispatch,
                self.block_info,
            );

            core_processor::handle_journal(journal, self);
        }
    }
}

impl JournalHandler for System {
    fn message_dispatched(&mut self, outcome: DispatchOutcome) {
        self.failed = matches!(
            outcome,
            DispatchOutcome::MessageTrap { .. } | DispatchOutcome::InitFailure { .. }
        );
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
        if let Some(prog) = self.programs.get_mut(&program_id) {
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
        if let Some(prog) = self.programs.get_mut(&program_id) {
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

pub struct TestProgram<'a> {
    system: &'a Mutex<System>,
    id: ProgramId,
}

impl TestSystem {
    pub fn assert_log<E: Encode>(&self, from: u64, payload: E) {
        self.assert_log_bytes(from, payload.encode())
    }
    pub fn assert_log_bytes<T: AsRef<[u8]>>(&self, from: u64, payload: T) {
        let system = self.0.lock().unwrap();
        let source = ProgramId::from(from);

        for log in &system.log {
            if log.source() == source && log.payload() == payload.as_ref().to_vec() {
                return;
            }
        }

        panic!("Log not found");
    }
    pub fn new() -> Self {
        Default::default()
    }

    pub fn send_message(&self, message: Message) {
        self.0.lock().unwrap().send_message(message)
    }

    pub fn set_user(&self, user: u64) {
        let mut system = self.0.lock().unwrap();
        if system.programs.contains_key(&ProgramId::from(user)) {
            panic!(
                "Can't set user {:?}, because Program with this id already exists",
                user
            )
        }

        system.user = user;
    }

    pub fn program_from_file<P: AsRef<Path>>(&self, path: P) -> TestProgram {
        let nonce = self.0.lock().unwrap().fetch_inc_id_nonce();

        self.program_from_file_with_id(nonce, path)
    }
    pub fn program_from_file_with_id<P: AsRef<Path>>(&self, id: u64, path: P) -> TestProgram {
        let code =
            fs::read(&path).unwrap_or_else(|_| panic!("Failed to find file {:?}", path.as_ref()));

        let program_id = ProgramId::from(id);
        let program = Program::new(program_id, code).expect("Failed to create Program from code");

        let mut system = self.0.lock().unwrap();

        if system.programs.insert(program.id(), program).is_some() {
            panic!(
                "Can't create program with id {:?}, because Program with this id already exists",
                id
            )
        }

        TestProgram {
            system: &self.0,
            id: program_id,
        }
    }
}

impl<'a> TestProgram<'a> {
    pub fn send<E: Encode>(&self, payload: E) {
        self.send_bytes(payload.encode())
    }

    pub fn send_bytes<T: AsRef<[u8]>>(&self, payload: T) {
        let mut system = self.system.lock().unwrap();

        let message = Message::new(
            MessageId::from(system.fetch_inc_message_nonce()),
            ProgramId::from(system.user),
            self.id,
            payload.as_ref().to_vec().into(),
            u64::MAX,
            0,
        );

        system.send_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn prepare_ping_pong<'a>(system: &'a TestSystem) -> TestProgram<'a> {
        let ping =
            system.program_from_file("../target/wasm32-unknown-unknown/release/demo_ping.wasm");
        ping.send_bytes("INIT");
        ping
    }
    #[test]
    fn single_ping() {
        let system = TestSystem::new();
        let ping = prepare_ping_pong(&system);
        ping.send_bytes("PING");
        system.assert_log_bytes(1, "PONG")
    }
    #[test]
    fn double_ping() {
        let system = TestSystem::new();
        let ping = prepare_ping_pong(&system);
        ping.send_bytes("PING");
        system.assert_log_bytes(1, "PONG");
        ping.send_bytes("PING");
        system.assert_log_bytes(1, "PONG")
    }
    #[test]
    #[should_panic(expected = "Log not found")]
    fn incorrect_ping() {
        let system = TestSystem::new();
        let ping = prepare_ping_pong(&system);
        ping.send_bytes("NOTHING");
        system.assert_log_bytes(1, "PONG")
    }
}
