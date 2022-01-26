use crate::{log::RunResult, manager::ExtManager, program::ProgramIdWrapper};
use colored::Colorize;
use env_logger::{Builder, Env};
use gear_core::message::Message;
use std::{cell::RefCell, fmt::Debug, io::Write, thread};

pub struct System(pub(crate) RefCell<ExtManager>);

impl Default for System {
    fn default() -> Self {
        Self(RefCell::new(ExtManager::new()))
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

    pub fn send_message(&self, message: Message) -> RunResult {
        self.0.borrow_mut().run_message(message)
    }

    pub fn spend_blocks(&self, amount: u32) {
        self.0.borrow_mut().block_info.height += amount;
        self.0.borrow_mut().block_info.timestamp += amount as u64;
    }

    pub fn set_user<I: Into<ProgramIdWrapper> + Clone + Debug>(&self, user: I) {
        let program_id = user.clone().into().0;

        let mut system = self.0.borrow_mut();

        if system.programs.contains_key(&program_id) {
            panic!(
                "Can't set user {:?}, because Program with this id already exists",
                user
            )
        }

        system.user = program_id;
    }

    pub fn user(&self) -> ProgramIdWrapper {
        ProgramIdWrapper(self.0.borrow().user)
    }
}
