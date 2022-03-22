use crate::{
    log::RunResult,
    manager::{Actor, ExtManager, Program as InnerProgram},
    system::System,
};
use codec::Codec;
use gear_core::{
    message::{Message, MessageId},
    program::{CheckedCode, Program as CoreProgram, ProgramId},
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
        let current_dir = env::current_dir().expect("Unable to get current dir");
        let path_file = current_dir.join(".binpath");
        let path_bytes = fs::read(path_file).expect("Unable to read path bytes");
        let relative_path: PathBuf = String::from_utf8(path_bytes).expect("Invalid path").into();
        let path = current_dir.join(relative_path);

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
        let code = CheckedCode::try_new(code).expect("Failed to create Program from code");
        let program = CoreProgram::new(program_id, code);

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
    use gear_core::message::Message;

    use crate::{CoreLog, System};

    use super::{Program, ProgramIdWrapper};

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
}
