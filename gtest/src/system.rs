use crate::{manager::ExtManager, program::ProgramIdWrapper};
use codec::Encode;
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::{message::Message, program::ProgramId};
use std::{fmt::Debug, io::Write, sync::Mutex, thread};

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
        let _ = Builder::from_env(Env::default().default_filter_or("gwasm=debug"))
            .format(|buf, record| {
                writeln!(
                    buf,
                    "[{} {}] {}",
                    record.level().to_string().blue(),
                    thread::current().name().unwrap_or("unknown").white(),
                    record.args().to_string().replacen("DEBUG: ", "", 1).white()
                )
            })
            .format_target(false)
            .format_timestamp(None)
            .try_init();
    }

    pub fn send_message(&self, message: Message) {
        self.0.lock().unwrap().run_message(message)
    }

    pub fn spend_blocks(&self, amount: u32) {
        self.0.lock().unwrap().block_info.height += amount;
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

    pub fn assert_run_success(&self) {
        if self.0.lock().unwrap().failed {
            panic!("Last run was failed!");
        }
    }

    pub fn assert_run_failed(&self) {
        if !self.0.lock().unwrap().failed {
            panic!("Last run was success!");
        }
    }
}
