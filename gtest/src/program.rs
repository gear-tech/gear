use crate::{
    log::RunResult,
    manager::{Actor, ExtManager, Program as InnerProgram},
    system::System,
};
use codec::Codec;
use gear_core::{
    message::{Message, MessageId},
    program::{Program as CoreProgram, ProgramId},
};
use path_clean::PathClean;
use std::{
    cell::RefCell,
    env,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

pub trait WasmProgram: Debug {
    fn init(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn handle(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn handle_reply(&mut self, payload: Vec<u8>) -> Result<Option<Vec<u8>>, &'static str>;
    fn debug(&mut self, data: &str) {
        logger::debug!(target: "gwasm", "DEBUG: {}", data);
    }
}

#[derive(Clone, Debug)]
pub struct ProgramIdWrapper(pub(crate) ProgramId);

impl<T: Into<ProgramIdWrapper> + Clone> PartialEq<T> for ProgramIdWrapper {
    fn eq(&self, other: &T) -> bool {
        self.0.eq(&other.clone().into().0)
    }
}

impl From<ProgramId> for ProgramIdWrapper {
    fn from(other: ProgramId) -> Self {
        Self(other)
    }
}

impl From<u64> for ProgramIdWrapper {
    fn from(other: u64) -> Self {
        Self(other.into())
    }
}

impl From<[u8; 32]> for ProgramIdWrapper {
    fn from(other: [u8; 32]) -> Self {
        Self(other.into())
    }
}

impl From<String> for ProgramIdWrapper {
    fn from(other: String) -> Self {
        other[..].into()
    }
}

impl From<&str> for ProgramIdWrapper {
    fn from(other: &str) -> Self {
        let id = other.strip_prefix("0x").unwrap_or(other);

        let mut bytes = [0u8; 32];

        if hex::decode_to_slice(id, &mut bytes).is_err() {
            panic!("Invalid identifier: {:?}", other)
        }

        Self(bytes.into())
    }
}

pub struct Program<'a> {
    pub(crate) manager: &'a RefCell<ExtManager>,
    pub(crate) id: ProgramId,
}

impl<'a> Program<'a> {
    fn program_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        program: InnerProgram,
    ) -> Self {
        let program_id = id.clone().into().0;

        if system
            .0
            .borrow_mut()
            .actors
            .insert(program_id, (Actor::new(program), 0))
            .is_some()
        {
            panic!(
                "Can't create program with id {:?}, because Program with this id already exists",
                id
            )
        }

        Self {
            manager: &system.0,
            id: program_id,
        }
    }

    pub fn current(system: &'a System) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::current_with_id(system, nonce)
    }

    pub fn current_with_id<I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
    ) -> Self {
        let path_file = env::var("OUT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| env::current_dir().expect("Unable to get current dir"));
        let path_file = path_file.join("wasm_binary_path.txt");
        let path_bytes = fs::read(path_file).expect("Unable to read path bytes");
        let path = String::from_utf8(path_bytes).expect("Invalid path");

        Self::from_file_with_id(system, id, path)
    }

    pub fn mock<T: WasmProgram + 'static>(system: &'a System, mock: T) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::mock_with_id(system, nonce, mock)
    }

    pub fn mock_with_id<T: WasmProgram + 'static, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        mock: T,
    ) -> Self {
        Self::program_with_id(system, id, InnerProgram::new_mock(mock))
    }

    pub fn from_file<P: AsRef<Path>>(system: &'a System, path: P) -> Self {
        let nonce = system.0.borrow_mut().free_id_nonce();

        Self::from_file_with_id(system, nonce, path)
    }

    pub fn from_file_with_id<P: AsRef<Path>, I: Into<ProgramIdWrapper> + Clone + Debug>(
        system: &'a System,
        id: I,
        path: P,
    ) -> Self {
        let path = env::current_dir()
            .expect("Unable to get root directory of the project")
            .join(path)
            .clean();

        let program_id = id.clone().into().0;

        let code = fs::read(&path).unwrap_or_else(|_| panic!("Failed to read file {:?}", path));

        let program =
            CoreProgram::new(program_id, code).expect("Failed to create Program from code");

        Self::program_with_id(system, id, InnerProgram::new(program))
    }

    pub fn send<ID: Into<ProgramIdWrapper>, C: Codec>(&self, from: ID, payload: C) -> RunResult {
        self.send_with_value(from, payload, 0)
    }

    pub fn send_with_value<ID: Into<ProgramIdWrapper>, C: Codec>(
        &self,
        from: ID,
        payload: C,
        value: u128,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload.encode(), value)
    }

    pub fn send_bytes<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
    ) -> RunResult {
        self.send_bytes_with_value(from, payload, 0)
    }

    pub fn send_bytes_with_value<ID: Into<ProgramIdWrapper>, T: AsRef<[u8]>>(
        &self,
        from: ID,
        payload: T,
        value: u128,
    ) -> RunResult {
        let mut system = self.manager.borrow_mut();

        let source = from.into().0;

        if system.actors.contains_key(&source) {
            panic!("Sending messages allowed only from users id");
        }

        if 0 < value && value < crate::EXISTENTIAL_DEPOSIT {
            panic!(
                "Value greater than 0, but less than required existential deposit ({})",
                crate::EXISTENTIAL_DEPOSIT
            );
        }

        let message = Message::new(
            MessageId::from(system.fetch_inc_message_nonce()),
            source,
            self.id,
            payload.as_ref().to_vec().into(),
            Some(u64::MAX),
            value,
        );

        system.run_message(message)
    }

    pub fn id(&self) -> ProgramId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use gear_core::message::{Message, MessageId, Payload};

    use super::{Program, ProgramIdWrapper};
    use crate::{CoreLog, Log, System};

    #[test]
    fn test_handle_messages_to_failing_program() {
        let sys = System::new();
        sys.init_logger();

        let user_id = 100;

        let prog = Program::from_file(
            &sys,
            "../target/wasm32-unknown-unknown/release/demo_futures_unordered.wasm",
        );

        let init_msg_payload = String::from("InvalidInput");
        let run_result = prog.send(user_id, init_msg_payload);
        assert!(run_result.main_failed);

        let expected_log = {
            // id, payload, gas limit, value and reply id aren't important
            let msg = Message::new_reply(
                Default::default(),
                prog.id(),
                ProgramIdWrapper::from(user_id).0,
                Default::default(),
                0,
                Default::default(),
                2,
            );
            CoreLog::from_message(msg)
        };
        let run_result = prog.send(user_id, String::from("should_be_skipped"));
        assert!(!run_result.main_failed());
        assert!(run_result.log.contains(&expected_log));
    }

    #[test]
    fn mailbox_mock_walkthrough_test() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let reply_payload: Payload = vec![3, 2, 1].into();
        let new_payload = message_payload.clone();
        let log = Log::builder().payload(new_payload);

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.clone(),
            Default::default(),
            2,
        );

        let message_result = system.send_message(message.clone());
        let message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        let mut destination_user_mailbox = system.get_mailbox(&destination_user_id);
        let message_replier = destination_user_mailbox
            .take_message(log)
            .expect("No message with such payload");
        let reply_result = message_replier.reply(reply_payload.clone(), 1);

        let reply_log = reply_result.expect("No message to reply to").log;
        let last_reply_log = reply_log.last().expect("No message log in run result");

        let second_message_result = system.send_message(message);

        let second_message_log = message_result
            .log
            .last()
            .expect("No message log in run result");

        assert!(!message_result.main_failed);
        assert!(!message_result.others_failed);
        assert!(!second_message_result.main_failed);
        assert!(!second_message_result.others_failed);
        assert_eq!(reply_log.len(), 1);
        assert_eq!(last_reply_log.get_payload(), reply_payload);
        assert_eq!(message_log.get_payload(), message_payload);
        assert_eq!(second_message_log.get_payload(), message_payload);
    }

    #[test]
    fn mailbox_mock_deletes_message_after_reply() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let reply_payload: Payload = vec![3, 2, 1].into();
        let log = Log::builder().payload(message_payload.clone());

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.clone(),
            Default::default(),
            2,
        );

        system.send_message(message.clone());

        let mut destination_user_mailbox = system.get_mailbox(&destination_user_id);
        let message_replier = destination_user_mailbox
            .take_message(log.clone())
            .expect("No message with such payload");
        message_replier.reply(reply_payload.clone(), 1);

        destination_user_mailbox = system.get_mailbox(&destination_user_id);
        let message_replier = destination_user_mailbox.take_message(log);

        assert!(message_replier.is_none())
    }

    #[test]
    fn mailbox_mock_reply_bytes_test() {
        let system = System::new();
        let message_id: MessageId = Default::default();
        let source_user_id = ProgramIdWrapper::from(100).0;
        let destination_user_id = ProgramIdWrapper::from(200).0;
        let message_payload: Payload = vec![1, 2, 3].into();
        let reply_payload: [u8; 3] = [3, 2, 1];
        let log = Log::builder().payload(message_payload.clone());

        let message = Message::new(
            message_id,
            source_user_id,
            destination_user_id,
            message_payload.clone(),
            Default::default(),
            2,
        );

        system.send_message(message.clone());

        let mut destination_user_mailbox = system.get_mailbox(&destination_user_id);
        let message_replier = destination_user_mailbox
            .take_message(log)
            .expect("No message with such payload");

        let result = message_replier.reply_bytes(&reply_payload, 1);
        let result_log = result.expect("No message to reply to").log;
        let last_result_log = result_log.last().expect("No message log in run result");

        assert_eq!(
            last_result_log.get_payload().into_raw(),
            reply_payload.to_vec()
        );
    }
}
