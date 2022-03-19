use crate::{log::RunResult, manager::{ExtManager, Mailbox}, program::{Program, ProgramIdWrapper}};
use colored::Colorize;
use env_logger::{Builder, Env};
use std::{cell::RefCell, io::Write, thread};
use gear_core::{
    message::Message,
    program::ProgramId
};

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
                let lvl = record.level().to_string().to_uppercase();
                let target = record.target().to_string();
                let mut msg = record.args().to_string();

                if target == "gwasm" {
                    msg = msg.replacen("DEBUG: ", "", 1);

                    writeln!(
                        buf,
                        "[{} {}] {}",
                        lvl.blue(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                } else {
                    writeln!(
                        buf,
                        "[{} {}] {}",
                        target.red(),
                        thread::current().name().unwrap_or("unknown").white(),
                        msg.white()
                    )
                }
            })
            .format_target(false)
            .format_timestamp(None)
            .try_init();
    }

    pub fn send_message(&self, message: Message) -> RunResult {
         self.0.borrow_mut().run_message(message.clone())
    }

    pub fn spend_blocks(&self, amount: u32) {
        self.0.borrow_mut().block_info.height += amount;
        self.0.borrow_mut().block_info.timestamp += amount as u64;
    }

    pub fn get_program<ID: Into<ProgramIdWrapper>>(&'_ self, id: ID) -> Program<'_> {
        Program {
            manager: &self.0,
            id: id.into().0,
        }
    }

    pub fn get_mailbox(&mut self, program_id: &ProgramId) -> Option<&Mailbox>{
        self.0.get_mut().get_mailbox(program_id)
    }

     pub(crate) fn fetch_inc_message_nonce(&self) -> u64 {
         self.0.borrow_mut().fetch_inc_message_nonce()
     }
}
