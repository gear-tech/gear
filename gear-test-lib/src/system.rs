use codec::Encode;
use env_logger::{Builder, Env};
use gear_core::{message::Message, program::ProgramId};
use std::{fmt::Debug, sync::Mutex};

use crate::{manager::ExtManager, program::ProgramIdWrapper};

pub struct System(pub(crate) Mutex<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(Mutex::new(ExtManager::new()))
    }
}

impl System {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn init_logger(&self) {
        let _ = env_logger::try_init();
    }

    pub fn init_wasm_logger(&self) {
        let _ = Builder::from_env(Env::default().default_filter_or("gwasm=debug")).try_init();
    }

    pub fn send_message(&self, message: Message) {
        self.0.lock().unwrap().run_message(message)
    }

    pub fn set_user<I: Into<ProgramIdWrapper> + Clone + Debug>(&self, user: I) {
        let program_id: ProgramId = user.clone().into().into();

        let mut system = self.0.lock().unwrap();

        if system.programs.contains_key(&program_id) {
            panic!(
                "Can't set user {:?}, because Program with this id already exists",
                user
            )
        }

        system.user = program_id;
    }

    pub fn assert_user<I: Into<ProgramIdWrapper> + Clone + Debug>(&self, user: I) {
        let program_id: ProgramId = user.clone().into().into();

        if self.0.lock().unwrap().user != program_id {
            panic!("User {:?} isn't actual user", user)
        }
    }

    pub fn get_user(&self) -> ProgramId {
        self.0.lock().unwrap().user
    }

    pub fn assert_log<E: Encode>(&self, from: u64, payload: E) {
        self.assert_log_bytes(from, payload.encode())
    }

    pub fn assert_log_bytes<T: AsRef<[u8]>>(&self, from: u64, payload: T) {
        let manager = self.0.lock().unwrap();
        let source = ProgramId::from(from);

        for log in &manager.log {
            if log.source() == source && log.payload() == payload.as_ref().to_vec() {
                return;
            }
        }

        panic!("Log not found");
    }

    pub fn assert_log_empty(&self) {
        if !self.0.lock().unwrap().log.is_empty() {
            panic!("Log is not empty");
        }
    }
}
