use codec::Encode;
use gear_core::{message::Message, program::ProgramId};
use std::sync::Mutex;

use crate::manager::ExtManager;

pub struct System(pub(crate) Mutex<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(Mutex::new(ExtManager {
            msg_nonce: 1,
            id_nonce: 1,
            user: 100001,
            ..Default::default()
        }))
    }
}

impl System {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn send_message(&self, message: Message) {
        self.0.lock().unwrap().run_message(message)
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

    pub fn get_user(&self) -> u64 {
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
